use tauri::{command, State, Manager, Emitter};
use std::sync::{Mutex, Arc};
use crate::network::ConnectionStatus;
use crate::logger::LogEntry;
use crate::clipboard::ClipboardListener;
use crate::network::NetworkManager;
use crate::config::AppConfig;
use crate::protocol::*;

pub struct AppState {
    pub network: Arc<Mutex<NetworkManager>>,
    pub logs: Arc<Mutex<Vec<LogEntry>>>,
    pub clipboard_listener: Mutex<Option<ClipboardListener>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            network: Arc::new(Mutex::new(NetworkManager::new())),
            logs: Arc::new(Mutex::new(Vec::new())),
            clipboard_listener: Mutex::new(None),
        }
    }
}

#[command]
pub fn get_config() -> AppConfig {
    AppConfig::load()
}

#[command]
pub fn save_config(config: AppConfig) -> Result<(), String> {
    config.save()
}

#[command]
pub fn get_connection_status(state: State<AppState>) -> String {
    let status = state.network.lock().unwrap().get_status();
    match status {
        ConnectionStatus::Connected => "connected".to_string(),
        ConnectionStatus::Connecting => "connecting".to_string(),
        ConnectionStatus::Disconnected => "disconnected".to_string(),
    }
}

#[command]
pub fn start_server(port: u16, window: tauri::Window, state: State<AppState>) -> Result<(), String> {
    let network = state.network.lock().unwrap();
    let logs = Arc::clone(&state.logs);
    let window_clone = window.clone();

    network.start_server(port, move |data, msg_type| {
        let (data_type, content, size) = match msg_type {
            TYPE_TEXT => ("text", String::from_utf8_lossy(&data).to_string(), data.len() as u64),
            TYPE_IMAGE => ("image", "截图".to_string(), data.len() as u64),
            TYPE_FILE => ("file", "文件".to_string(), data.len() as u64),
            _ => ("", "未知".to_string(), 0),
        };

        let entry = LogEntry::recv(data_type, &content, size);
        entry.write_to_file().ok();
        logs.lock().unwrap().push(entry.clone());
        let _ = window_clone.app_handle().emit("clipboard-received", &entry);
    })
}

#[command]
pub fn connect_to_server(ip: String, port: u16, window: tauri::Window, state: State<AppState>) -> Result<(), String> {
    let network = state.network.lock().unwrap();
    let logs = Arc::clone(&state.logs);
    let window_clone = window.clone();

    network.connect_to_server(&ip, port, move |data, msg_type| {
        let (data_type, content, size) = match msg_type {
            TYPE_TEXT => ("text", String::from_utf8_lossy(&data).to_string(), data.len() as u64),
            TYPE_IMAGE => ("image", "截图".to_string(), data.len() as u64),
            TYPE_FILE => ("file", "文件".to_string(), data.len() as u64),
            _ => ("", "未知".to_string(), 0),
        };

        let entry = LogEntry::recv(data_type, &content, size);
        entry.write_to_file().ok();
        logs.lock().unwrap().push(entry.clone());
        let _ = window_clone.app_handle().emit("clipboard-received", &entry);
    })
}

#[command]
pub fn disconnect(state: State<AppState>) {
    state.network.lock().unwrap().disconnect();
}

#[command]
pub fn send_text(text: String, state: State<AppState>) -> Result<(), String> {
    let network = state.network.lock().unwrap();
    network.send_text(&text)?;
    let entry = LogEntry::send("text", &text, text.len() as u64);
    entry.write_to_file().ok();
    state.logs.lock().unwrap().push(entry);
    Ok(())
}

#[command]
pub fn send_image(data: Vec<u8>, state: State<AppState>) -> Result<(), String> {
    let network = state.network.lock().unwrap();
    network.send_image(&data)?;
    let entry = LogEntry::send("image", "截图", data.len() as u64);
    entry.write_to_file().ok();
    state.logs.lock().unwrap().push(entry);
    Ok(())
}

#[command]
pub fn get_logs(state: State<AppState>) -> Vec<LogEntry> {
    state.logs.lock().unwrap().clone()
}

#[command]
pub fn clear_logs(state: State<AppState>) {
    state.logs.lock().unwrap().clear();
}

#[command]
pub fn start_clipboard_monitor(window: tauri::Window, state: State<AppState>) -> Result<(), String> {
    let listener = ClipboardListener::new();
    let logs = Arc::clone(&state.logs);
    let network: Arc<Mutex<NetworkManager>> = Arc::clone(&state.network);

    listener.start(move |content| {
        let data_type = content.data_type();
        let summary = content.summary();
        let size = content.size();

        let entry = LogEntry::send(data_type, &summary, size);
        entry.write_to_file().ok();
        logs.lock().unwrap().push(entry.clone());
        let _ = window.app_handle().emit("clipboard-changed", &entry);

        if let Ok(net) = network.lock() {
            match content {
                crate::clipboard::ClipboardContent::Text(t) => { net.send_text(&t).ok(); }
                crate::clipboard::ClipboardContent::Image(b) => { net.send_image(&b).ok(); }
                crate::clipboard::ClipboardContent::Files(_) => { }
            }
        }
    });

    *state.clipboard_listener.lock().unwrap() = Some(listener);
    Ok(())
}
