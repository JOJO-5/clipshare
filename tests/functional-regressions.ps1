$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$configPanel = Get-Content -LiteralPath (Join-Path $repoRoot 'src\components\ConfigPanel.tsx') -Raw
$commands = Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri\src\commands.rs') -Raw
$clipboard = Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri\src\clipboard.rs') -Raw
$network = Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri\src\network.rs') -Raw
$protocol = Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri\src\protocol.rs') -Raw
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
if ($network -notmatch 'send_with_ack\(TYPE_TEXT' -or $network -notmatch 'send_with_ack\(TYPE_IMAGE' -or $network -notmatch 'send_with_ack\(TYPE_FILE') {
    throw 'Text, image, and file clipboard payloads must wait for a processing acknowledgement.'
}
if ($network -notmatch 'processed_sequences' -or $network -notmatch 'TYPE_NACK') {
    throw 'Received payloads must be deduplicated by sequence and report processing failures.'
}
if ($commands -notmatch 'ClipboardSuppression' -or $commands -notmatch 'apply_received_clipboard') {
    throw 'Remote clipboard writes must be marked to prevent echoing back to the sender.'
}
if ($clipboard -notmatch 'origin=remote status=suppressed') {
    throw 'Clipboard monitoring must log and suppress remote-origin clipboard changes.'
}
if ($protocol -notmatch 'TYPE_NACK') {
    throw 'The protocol must reserve a negative acknowledgement message type.'
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
if ($wechatMonitor -notmatch 'UIInvokePattern' -or $wechatMonitor -notmatch 'UISelectionItemPattern') {
    throw 'WeChat monitoring must prefer non-mouse UIA activation for background windows.'
}
if ($wechatMonitor -notmatch 'GetForegroundWindow' -or $wechatMonitor -notmatch 'IsIconic') {
    throw 'Mouse fallback must be restricted to an interactive foreground WeChat window.'
}
if ($wechatMonitor -notmatch 'SESSION_CLICK_RETRY_INTERVAL') {
    throw 'Unread session activation must retry when the unread indicator remains visible.'
}
if ($wechatMonitor -notmatch 'should_retry_mouse_after_activation') {
    throw 'UIA activation must be verified and retried with the foreground mouse fallback when no message nodes appear.'
}
$mouseActivation = $wechatMonitor.IndexOf('can_use_mouse_fallback(window) && item.click().is_ok()')
$uiaActivation = $wechatMonitor.IndexOf('get_pattern::<UIInvokePattern>()')
if ($mouseActivation -lt 0 -or $uiaActivation -lt 0 -or $mouseActivation -gt $uiaActivation) {
    throw 'Foreground WeChat sessions must prefer the original physical click before UIA activation.'
}
$mouseAvailability = $wechatMonitor.IndexOf('let initial_mouse_fallback_available = can_use_mouse_fallback(window)')
$activationCall = $wechatMonitor.IndexOf('let activation_method = activate_unread_session')
if ($mouseAvailability -lt 0 -or $activationCall -lt 0 -or $mouseAvailability -gt $activationCall) {
    throw 'Mouse fallback availability must be captured before UIA can activate a background WeChat window.'
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
if ($commands -notmatch 'max_by_key') {
    throw 'Pending WeChat startup recovery must choose the newest valid queue candidate.'
}
if ($commands -notmatch 'wechat-pending-save failed' -or $commands -notmatch 'wechat-pending-load failed') {
    throw 'Pending WeChat persistence failures must be logged.'
}
$app = Get-Content -LiteralPath (Join-Path $repoRoot 'src\App.tsx') -Raw
if ($app -notmatch 'wechatNotificationKeys') {
    throw 'WeChat notifications must suppress duplicate native toasts.'
}
if ($app -notmatch 'wechatNotificationGroups' -or $app -notmatch 'key: groupKey') {
    throw 'WeChat notifications must collapse multiple messages from one sender.'
}
if ($app -match 'hasPendingNotification') {
    throw 'A pending notification must not permanently suppress later WeChat notifications.'
}
if ($app -notmatch 'WECHAT_NATIVE_NOTIFICATION_COOLDOWN_MS' -or $app -notmatch 'shouldSendNativeWechatNotification') {
    throw 'WeChat native notifications must use a repeatable cooldown instead of permanent pending suppression.'
}
if ($app -notmatch 'wechat-notification permission=' -or $app -notmatch 'wechat-notification status=sent' -or $app -notmatch 'wechat-notification status=failed') {
    throw 'WeChat notification permission and send outcomes must be visible in diagnostics.'
}
$subscribeCall = $app.LastIndexOf('subscribeToLogs()')
$wechatMonitorStart = $app.IndexOf("invoke('start_wechat_monitor')")
if ($subscribeCall -lt 0 -or $wechatMonitorStart -lt 0 -or $wechatMonitorStart -lt $subscribeCall) {
    throw 'WeChat monitor must start only after frontend event listeners are registered.'
}
if ($app -match "message\.id\.startsWith\('wechat-unread-'\)") {
    throw 'Full unread messages must keep stable occurrence IDs instead of semantic deduplication.'
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
