$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$configPanel = Get-Content -LiteralPath (Join-Path $repoRoot 'src\components\ConfigPanel.tsx') -Raw
$commands = Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri\src\commands.rs') -Raw
$clipboard = Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri\src\clipboard.rs') -Raw
$wechatMonitor = Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri\src\wechat_monitor.rs') -Raw
$workflow = Get-Content -LiteralPath (Join-Path $repoRoot '.github\workflows\build.yml') -Raw
$packageJson = Get-Content -LiteralPath (Join-Path $repoRoot 'package.json') -Raw

if ($configPanel -notmatch 'getFieldsValue\(true\)') {
    throw 'Config save must include unmounted fields such as port.'
}
if ($commands -notmatch 'set_files\(' -or $clipboard -notmatch 'FileList[\s\S]*write_clipboard') {
    throw 'Received files must be written back as a Windows file-list clipboard payload.'
}
if ($wechatMonitor -notmatch 'ChatWnd') {
    throw 'WeChat monitor must recognize top-level ChatWnd windows.'
}
if ($workflow -match 'cargo install tauri-cli') {
    throw 'CI must use the npm-installed Tauri CLI instead of compiling tauri-cli with Cargo.'
}
if ($packageJson -notmatch '"@tauri-apps/cli"') {
    throw 'CI must install the Tauri CLI through package-lock via npm ci.'
}

Write-Output 'Functional regression contracts are present.'
