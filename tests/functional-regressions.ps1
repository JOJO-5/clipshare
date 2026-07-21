$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$configPanel = Get-Content -LiteralPath (Join-Path $repoRoot 'src\components\ConfigPanel.tsx') -Raw
$commands = Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri\src\commands.rs') -Raw
$clipboard = Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri\src\clipboard.rs') -Raw
$wechatMonitor = Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri\src\wechat_monitor.rs') -Raw

if ($configPanel -notmatch 'getFieldsValue\(true\)') {
    throw 'Config save must include unmounted fields such as port.'
}
if ($commands -notmatch 'set_files\(' -or $clipboard -notmatch 'FileList[\s\S]*write_clipboard') {
    throw 'Received files must be written back as a Windows file-list clipboard payload.'
}
if ($wechatMonitor -notmatch 'ChatWnd') {
    throw 'WeChat monitor must recognize top-level ChatWnd windows.'
}

Write-Output 'Functional regression contracts are present.'
