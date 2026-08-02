pub mod clipboard;
pub mod commands;
pub mod config;
pub mod autostart;
pub mod logger;
pub mod network;
pub mod protocol;
pub mod wechat;
pub mod wechat_monitor;

use commands::*;
use tauri::{
    Manager,
    WindowEvent,
    menu::{Menu, MenuItem},
    tray::{TrayIconBuilder, MouseButton, MouseButtonState},
    image::Image,
};
use crate::config::AppConfig;

pub fn run() {
    let minimize_to_tray = AppConfig::load().minimize_to_tray;
    let start_minimized = std::env::args().any(|argument| argument == "--minimized");

    #[cfg(not(feature = "win7-compat"))]
    let builder = tauri::Builder::default().plugin(tauri_plugin_notification::init());
    #[cfg(feature = "win7-compat")]
    let builder = tauri::Builder::default();

    builder
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            get_connection_status,
            start_server,
            connect_to_server,
            disconnect,
            send_text,
            send_wechat,
            send_image,
            get_logs,
            clear_logs,
            start_clipboard_monitor,
            start_wechat_monitor,
        ])
        .setup(move |app| {
            // Build tray icon menu
            let show_item = MenuItem::with_id(app, "show", "打开主界面", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            // Load icon
            let icon = Image::from_bytes(include_bytes!("../icons/icon.png"))
                .unwrap_or_else(|_| Image::from_bytes(&[]).unwrap());

            // Create tray icon
            let _tray = TrayIconBuilder::new()
                .icon(icon)
                .menu(&menu)
                .tooltip("ClipShare 剪贴板共享")
                .on_menu_event(|app, event| {
                    match event.id.as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // Set window title
            let window = app.get_webview_window("main").unwrap();
            window.set_title("ClipShare").ok();
            if start_minimized {
                let _ = window.hide();
            }

            let state = app.state::<AppState>();
            if let Err(error) = restore_saved_connection(app.handle().clone(), state.inner()) {
                let entry =
                    crate::logger::LogEntry::error(&format!("network auto-start failed: {error}"));
                entry.write_to_file().ok();
                state.logs.lock().unwrap().push(entry);
            }

            Ok(())
        })
        .on_window_event(move |window, event| match event {
            WindowEvent::CloseRequested { api, .. } if minimize_to_tray => {
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::Resized(_) if minimize_to_tray => {
                if window.is_minimized().unwrap_or(false) {
                    let _ = window.hide();
                }
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
