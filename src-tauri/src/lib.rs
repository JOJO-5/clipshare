pub mod clipboard;
pub mod commands;
pub mod config;
pub mod autostart;
pub mod logger;
pub mod network;
pub mod protocol;
mod updates;
pub mod wechat;
pub mod wechat_monitor;

#[cfg(all(windows, feature = "win7-compat"))]
mod win7_webview;

use commands::*;
use std::sync::atomic::Ordering;
use tauri::{
    Manager,
    WindowEvent,
    menu::{Menu, MenuItem},
    tray::{TrayIconBuilder, MouseButton, MouseButtonState},
    image::Image,
};

pub fn run() {
    let context = tauri::generate_context!();
    #[cfg(all(windows, feature = "win7-compat"))]
    if let Err(error) = win7_webview::check_fixed_runtime(context.config()) {
        win7_webview::show_error(&error);
        std::process::exit(1);
    }

    let start_minimized = std::env::args().any(|argument| argument == "--minimized");

    #[cfg(not(feature = "win7-compat"))]
    let builder = tauri::Builder::default().plugin(tauri_plugin_notification::init());
    #[cfg(feature = "win7-compat")]
    let builder = tauri::Builder::default();

    #[cfg(windows)]
    let builder = builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updates::UpdateInstallLock::default());

    builder
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            save_preferences,
            get_connection_status,
            get_connection_info,
            get_local_addresses,
            copy_local_text,
            updates::get_update_info,
            updates::open_release_downloads,
            #[cfg(windows)]
            updates::check_for_update,
            #[cfg(windows)]
            updates::install_app_update,
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
            set_wechat_monitor_enabled,
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
        .on_window_event(|window, event| {
            if !window.state::<AppState>().minimize_to_tray.load(Ordering::Relaxed) {
                return;
            }
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window.hide();
                }
                WindowEvent::Resized(_) if window.is_minimized().unwrap_or(false) => {
                    let _ = window.hide();
                }
                _ => {}
            }
        })
        .run(context)
        .expect("error while running tauri application");
}
