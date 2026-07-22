use crate::autostart::set_autostart;
use crate::clipboard::ClipboardListener;
use crate::config::AppConfig;
use crate::logger::LogEntry;
use crate::network::ConnectionStatus;
use crate::network::NetworkManager;
use crate::protocol::*;
use crate::wechat_monitor::WeChatMonitor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tauri::{command, Emitter, Manager, State};

pub struct AppState {
    pub network: Arc<Mutex<NetworkManager>>,
    pub logs: Arc<Mutex<Vec<LogEntry>>>,
    pub clipboard_listener: Mutex<Option<ClipboardListener>>,
    pub wechat_monitor: Mutex<Option<WeChatMonitor>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            network: Arc::new(Mutex::new(NetworkManager::new())),
            logs: Arc::new(Mutex::new(Vec::new())),
            clipboard_listener: Mutex::new(None),
            wechat_monitor: Mutex::new(None),
        }
    }
}

#[command]
pub fn get_config() -> AppConfig {
    AppConfig::load()
}

#[command]
pub fn save_config(config: AppConfig) -> Result<(), String> {
    set_autostart(config.autostart)?;
    config.save()
}

const MAX_RECEIVED_FILES: usize = 100;
const RECEIVED_FILE_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);

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
pub fn start_server(
    port: u16,
    window: tauri::Window,
    state: State<AppState>,
) -> Result<(), String> {
    let network = state.network.lock().unwrap();
    let logs = Arc::clone(&state.logs);
    let window_clone = window.clone();

    network.start_server(port, move |data, msg_type| {
        let (data_type, content, size) = match msg_type {
            TYPE_TEXT => {
                let text = String::from_utf8_lossy(&data).to_string();
                let _ = crate::clipboard::set_text(&text);
                ("text", text, data.len() as u64)
            }
            TYPE_IMAGE => match decode_image(&data) {
                Ok((width, height, bytes)) => {
                    let _ = crate::clipboard::set_image(width, height, bytes);
                    ("image", "图片".to_string(), data.len() as u64)
                }
                Err(error) => ("error", error, 0),
            },
            TYPE_FILE => match save_received_file(&data) {
                Ok(path) => (
                    "file",
                    format!("已保存: {}", path.display()),
                    data.len() as u64,
                ),
                Err(error) => ("error", error, 0),
            },
            TYPE_WECHAT => match decode_wechat(&data) {
                Ok(message) => ("wechat", message.preview, data.len() as u64),
                Err(error) => ("error", error, 0),
            },
            _ => ("", "未知".to_string(), 0),
        };

        let entry = LogEntry::recv(data_type, &content, size);
        entry.write_to_file().ok();
        logs.lock().unwrap().push(entry.clone());
        let _ = window_clone.app_handle().emit("clipboard-received", &entry);
        if msg_type == TYPE_WECHAT {
            if let Ok(message) = decode_wechat(&data) {
                let _ = window_clone.app_handle().emit("wechat-received", &message);
            }
        }
    })
}

#[command]
pub fn connect_to_server(
    ip: String,
    port: u16,
    window: tauri::Window,
    state: State<AppState>,
) -> Result<(), String> {
    let network = state.network.lock().unwrap();
    let logs = Arc::clone(&state.logs);
    let window_clone = window.clone();

    network.connect_to_server(&ip, port, move |data, msg_type| {
        let (data_type, content, size) = match msg_type {
            TYPE_TEXT => {
                let text = String::from_utf8_lossy(&data).to_string();
                let _ = crate::clipboard::set_text(&text);
                ("text", text, data.len() as u64)
            }
            TYPE_IMAGE => match decode_image(&data) {
                Ok((width, height, bytes)) => {
                    let _ = crate::clipboard::set_image(width, height, bytes);
                    ("image", "图片".to_string(), data.len() as u64)
                }
                Err(error) => ("error", error, 0),
            },
            TYPE_FILE => match save_received_file(&data) {
                Ok(path) => (
                    "file",
                    format!("已保存: {}", path.display()),
                    data.len() as u64,
                ),
                Err(error) => ("error", error, 0),
            },
            TYPE_WECHAT => match decode_wechat(&data) {
                Ok(message) => ("wechat", message.preview, data.len() as u64),
                Err(error) => ("error", error, 0),
            },
            _ => ("", "未知".to_string(), 0),
        };

        let entry = LogEntry::recv(data_type, &content, size);
        entry.write_to_file().ok();
        logs.lock().unwrap().push(entry.clone());
        let _ = window_clone.app_handle().emit("clipboard-received", &entry);
        if msg_type == TYPE_WECHAT {
            if let Ok(message) = decode_wechat(&data) {
                let _ = window_clone.app_handle().emit("wechat-received", &message);
            }
        }
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
pub fn send_wechat(message: WeChatMessage, state: State<AppState>) -> Result<(), String> {
    let network = state.network.lock().unwrap();
    network.send_wechat(&message)?;
    let entry = LogEntry::send("wechat", &message.preview, message.content.len() as u64);
    entry.write_to_file().ok();
    state.logs.lock().unwrap().push(entry);
    Ok(())
}

#[command]
pub fn start_wechat_monitor(window: tauri::Window, state: State<AppState>) -> Result<(), String> {
    let config = AppConfig::load();
    if !config.wechat_enabled {
        let entry = LogEntry::info("wechat-monitor", "wechat-monitor disabled by config");
        entry.write_to_file().ok();
        state.logs.lock().unwrap().push(entry.clone());
        let _ = window.app_handle().emit("wechat-monitor-log", &entry);
        return Ok(());
    }

    let mut monitor_slot = state.wechat_monitor.lock().unwrap();
    if monitor_slot.is_some() {
        return Ok(());
    }

    let logs = Arc::clone(&state.logs);
    let diagnostic_logs = Arc::clone(&state.logs);
    let network = Arc::clone(&state.network);
    let window_clone = window.clone();
    let diagnostic_window = window.clone();
    let preview_limit = config.wechat_preview_limit.max(1);
    let monitor = WeChatMonitor::start(
        move |message| {
            let message = prepare_wechat_message(message, preview_limit);
            let delivered = network
                .lock()
                .map_err(|_| "Network state is unavailable".to_string())
                .and_then(|net| {
                    if net.get_status() != ConnectionStatus::Connected {
                        return Err("Not connected".to_string());
                    }
                    net.send_wechat(&message)
                })
                .is_ok();

            if delivered {
                let entry =
                    LogEntry::send("wechat", &message.preview, message.content.len() as u64);
                entry.write_to_file().ok();
                logs.lock().unwrap().push(entry);
                let _ = window_clone.app_handle().emit("wechat-sent", &message);
            }
            delivered
        },
        move |diagnostic| {
            let entry = LogEntry::info("wechat-monitor", &diagnostic);
            entry.write_to_file().ok();
            diagnostic_logs.lock().unwrap().push(entry.clone());
            let _ = diagnostic_window
                .app_handle()
                .emit("wechat-monitor-log", &entry);
        },
    )?;

    *monitor_slot = Some(monitor);
    Ok(())
}

#[command]
pub fn send_image(
    width: usize,
    height: usize,
    data: Vec<u8>,
    state: State<AppState>,
) -> Result<(), String> {
    let network = state.network.lock().unwrap();
    network.send_image(width, height, &data)?;
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
pub fn start_clipboard_monitor(
    window: tauri::Window,
    state: State<AppState>,
) -> Result<(), String> {
    let mut listener_slot = state.clipboard_listener.lock().unwrap();
    if listener_slot.is_some() {
        return Ok(());
    }

    let listener = ClipboardListener::new();
    let logs = Arc::clone(&state.logs);
    let diagnostic_logs = Arc::clone(&state.logs);
    let network: Arc<Mutex<NetworkManager>> = Arc::clone(&state.network);
    let window_clone = window.clone();
    let diagnostic_window = window.clone();

    listener.start(
        move |content| {
            let data_type = content.data_type();
            let summary = content.summary();
            let size = content.size();

            let result = network
                .lock()
                .map_err(|_| "Network state is unavailable".to_string())
                .and_then(|net| send_clipboard_content(&net, content));

            if result.is_ok() {
                let entry = LogEntry::send(data_type, &summary, size);
                entry.write_to_file().ok();
                logs.lock().unwrap().push(entry.clone());
                let _ = window_clone.app_handle().emit("clipboard-changed", &entry);
            }
            result
        },
        move |diagnostic| {
            let entry = LogEntry::info("clipboard-monitor", &diagnostic);
            entry.write_to_file().ok();
            diagnostic_logs.lock().unwrap().push(entry.clone());
            let _ = diagnostic_window
                .app_handle()
                .emit("clipboard-monitor-log", &entry);
        },
    );

    *listener_slot = Some(listener);
    Ok(())
}

fn send_clipboard_content(
    network: &NetworkManager,
    content: crate::clipboard::ClipboardContent,
) -> Result<(), String> {
    if network.get_status() != ConnectionStatus::Connected {
        return Err("Not connected".to_string());
    }

    match content {
        crate::clipboard::ClipboardContent::Text(text) => network.send_text(&text),
        crate::clipboard::ClipboardContent::Image {
            width,
            height,
            bytes,
        } => network.send_image(width, height, &bytes),
        crate::clipboard::ClipboardContent::Files(paths) => {
            for path in paths {
                let file = std::path::Path::new(&path);
                let name = file
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| "Invalid file name".to_string())?;
                let contents = std::fs::read(file).map_err(|error| error.to_string())?;
                network.send_file_data(name, &contents)?;
            }
            Ok(())
        }
    }
}

fn save_received_file(payload: &[u8]) -> Result<std::path::PathBuf, String> {
    let (filename, contents) = decode_file(payload)?;
    let directory = dirs::download_dir()
        .unwrap_or_else(|| AppConfig::config_dir().join("downloads"))
        .join("ClipShare");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let target = directory.join(filename);
    std::fs::write(&target, contents).map_err(|error| error.to_string())?;
    crate::clipboard::set_files(&[target.to_string_lossy().into_owned()])?;
    cleanup_received_files(&directory, &target);
    Ok(target)
}

fn select_received_files_for_cleanup(
    mut files: Vec<(PathBuf, SystemTime)>,
    current_file: &Path,
    now: SystemTime,
) -> Vec<PathBuf> {
    files.sort_by_key(|(_, modified)| *modified);
    let keep_current = files.iter().any(|(path, _)| path == current_file);
    let max_other_files = MAX_RECEIVED_FILES.saturating_sub(usize::from(keep_current));
    let cutoff = now
        .checked_sub(RECEIVED_FILE_RETENTION)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let mut removals = Vec::new();
    let mut fresh_files = Vec::new();

    for (path, modified) in files {
        if path == current_file {
            continue;
        }
        if modified < cutoff {
            removals.push(path);
        } else {
            fresh_files.push(path);
        }
    }

    if fresh_files.len() > max_other_files {
        let excess = fresh_files.len() - max_other_files;
        removals.extend(fresh_files.into_iter().take(excess));
    }

    removals
}

fn cleanup_received_files(directory: &Path, current_file: &Path) {
    let files = match std::fs::read_dir(directory) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let file_type = entry.file_type().ok()?;
                if !file_type.is_file() {
                    return None;
                }
                let modified = entry.metadata().ok()?.modified().ok()?;
                Some((entry.path(), modified))
            })
            .collect(),
        Err(_) => return,
    };

    for path in select_received_files_for_cleanup(files, current_file, SystemTime::now()) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    #[test]
    fn clipboard_content_is_not_marked_delivered_when_peer_is_disconnected() {
        let network = NetworkManager::new();

        assert!(send_clipboard_content(
            &network,
            crate::clipboard::ClipboardContent::Text("pending transfer".to_string()),
        )
        .is_err());
    }

    #[test]
    fn received_file_cleanup_removes_expired_files_but_keeps_current_file() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);
        let current = std::path::PathBuf::from("current.txt");
        let expired = std::path::PathBuf::from("expired.txt");
        let fresh = std::path::PathBuf::from("fresh.txt");

        let removals = select_received_files_for_cleanup(
            vec![
                (current.clone(), now - Duration::from_secs(8 * 24 * 60 * 60)),
                (expired.clone(), now - Duration::from_secs(8 * 24 * 60 * 60)),
                (fresh, now - Duration::from_secs(60)),
            ],
            &current,
            now,
        );

        assert_eq!(removals, vec![expired]);
    }

    #[test]
    fn received_file_cleanup_caps_fresh_cache_at_one_hundred_files() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);
        let current = std::path::PathBuf::from("file-100.txt");
        let files = (0..=100)
            .map(|index| {
                (
                    std::path::PathBuf::from(format!("file-{index}.txt")),
                    now - Duration::from_secs(1_000 - index as u64),
                )
            })
            .collect();

        let removals = select_received_files_for_cleanup(files, &current, now);

        assert_eq!(removals, vec![std::path::PathBuf::from("file-0.txt")]);
    }
}
