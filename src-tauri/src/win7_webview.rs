use tauri::utils::{
    config::{Config, WebviewInstallMode},
    platform::current_exe,
};
use windows::core::{w, HSTRING};
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR};

pub fn check_fixed_runtime(config: &Config) -> Result<(), String> {
    let WebviewInstallMode::FixedRuntime { path } = &config.bundle.windows.webview_install_mode
    else {
        return Ok(());
    };

    let exe = current_exe().map_err(|error| format!("无法定位 ClipShare.exe。\n{error}"))?;
    let runtime = exe.with_file_name(path);
    let loader = runtime.join(r"EBWebView\x64\EmbeddedBrowserWebView.dll");
    if !loader.is_file() {
        return Err(format!(
            "找不到 WebView2 运行时 DLL。\n{}",
            loader.display()
        ));
    }

    std::env::set_var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", &runtime);
    tauri::webview_version().map_err(|error| {
        format!(
            "无法读取 WebView2 运行时版本。\n{}\n{error:?}",
            runtime.display()
        )
    })?;
    Ok(())
}

pub fn show_error(error: &str) {
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(error),
            w!("ClipShare Win7"),
            MB_ICONERROR,
        );
    }
}
