$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$configPanel = Get-Content -LiteralPath (Join-Path $repoRoot 'src\components\ConfigPanel.tsx') -Raw
$commands = Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri\src\commands.rs') -Raw
$clipboard = Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri\src\clipboard.rs') -Raw
$wechatMonitor = Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri\src\wechat_monitor.rs') -Raw
$workflow = Get-Content -LiteralPath (Join-Path $repoRoot '.github\workflows\build.yml') -Raw
$packageJson = Get-Content -LiteralPath (Join-Path $repoRoot 'package.json') -Raw
$cargoManifest = Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri\Cargo.toml') -Raw
$lib = Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri\src\lib.rs') -Raw
$config = Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri\src\config.rs') -Raw

if ($configPanel -notmatch 'getFieldsValue\(true\)') {
    throw 'Config save must include unmounted fields such as port.'
}
if ($commands -notmatch 'set_files\(' -or $clipboard -notmatch 'FileList[\s\S]*write_clipboard') {
    throw 'Received files must be written back as a Windows file-list clipboard payload.'
}
if ($wechatMonitor -notmatch 'ChatWnd') {
    throw 'WeChat monitor must recognize top-level ChatWnd windows.'
}
if ($wechatMonitor -notmatch 'AccessibleObjectFromWindow' -or $wechatMonitor -notmatch 'msaa_nodes') {
    throw 'WeChat monitor must retain the HWND/MSAA fallback and its remote diagnostics.'
}
if ($wechatMonitor -notmatch 'item\.click\(\)') {
    throw 'Active WeChat monitoring must open recognized unread sessions to read full messages.'
}
if ($wechatMonitor -notmatch 'should_monitor_session') {
    throw 'WeChat unread sessions must be filtered before active opening.'
}
if ($wechatMonitor -notmatch 'contains\(&keyword\)') {
    throw 'WeChat session filters must support fuzzy keyword matching.'
}
if ($wechatMonitor -notmatch 'SessionClickTracker') {
    throw 'WeChat monitoring must prevent repeated clicks while a session remains unread.'
}
if ($wechatMonitor -notmatch 'monitor_thread\.join\(\)') {
    throw 'Stopping WeChat monitoring must wait for its worker thread to exit.'
}
if ($commands -notmatch 'set_wechat_monitor_enabled') {
    throw 'The WeChat notification switch must reconfigure the running monitor immediately.'
}
if ($commands -notmatch 'wechat-send status=failed') {
    throw 'Failed WeChat notification delivery must be visible in diagnostics.'
}
if ($commands -notmatch 'pending_wechat_temp_path' -or $commands -notmatch 'pending_wechat_backup_path') {
    throw 'Pending WeChat notifications must use temporary and backup files for crash-safe persistence.'
}
if ($commands -notmatch 'pending_wechat_path\(\),\s*pending_wechat_backup_path\(\),\s*pending_wechat_temp_path') {
    throw 'Pending WeChat startup recovery must inspect the temporary file as well as the target and backup.'
}
if ($commands -notmatch 'wechat-pending-save failed' -or $commands -notmatch 'wechat-pending-load failed') {
    throw 'Pending WeChat persistence failures must be logged.'
}
if ($workflow -match 'cargo install tauri-cli') {
    throw 'CI must use the npm-installed Tauri CLI instead of compiling tauri-cli with Cargo.'
}
if ($workflow -notmatch 'pull_request:') {
    throw 'Pull requests must run the build workflow automatically.'
}
if ($workflow -notmatch 'win7-webview2-config\.ps1') {
    throw 'CI must run the Win7 WebView2 configuration regression test.'
}
if ($packageJson -notmatch '"@tauri-apps/cli"') {
    throw 'CI must install the Tauri CLI through package-lock via npm ci.'
}
if ($workflow -notmatch 'timeout-minutes:\s*30') {
    throw 'Each CI build job must have a finite timeout.'
}
if ($workflow -notmatch "nightly-2026-07-22") {
    throw 'The Win7 build must pin the validated Rust nightly toolchain.'
}
if ($workflow -notmatch 'check-win7-imports\.ps1') {
    throw 'CI must reject Win7 executables with unsupported imports.'
}
if ($cargoManifest -notmatch 'webview2-com-sys-0\.38\.2-win7') {
    throw 'Cargo must patch webview2-com-sys to use the loader that supports unpatched Windows 7.'
}
if ($lib -notmatch 'restore_saved_connection') {
    throw 'Application startup must restore the saved server/client connection role.'
}
if ($config -notmatch 'remember_client_connection' -or $config -notmatch 'remember_server_connection') {
    throw 'Manual connection actions must persist the endpoint used for the next startup.'
}

Write-Output 'Functional regression contracts are present.'
