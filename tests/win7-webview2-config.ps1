$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$configPath = Join-Path $repoRoot 'src-tauri\tauri.win7.conf.json'
$workflowPath = Join-Path $repoRoot '.github\workflows\build.yml'

$config = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
$installMode = $config.bundle.windows.webviewInstallMode
if ($installMode.type -ne 'fixedRuntime') {
    throw "Win7 must use fixedRuntime, found '$($installMode.type)'."
}
if ($installMode.path -notmatch 'webview2-runtime') {
    throw "Win7 fixed runtime path must point to webview2-runtime, found '$($installMode.path)'."
}

$workflow = Get-Content -LiteralPath $workflowPath -Raw
foreach ($requiredText in @(
    'webview2.runtime.x64.109.0.1518.78.nupkg',
    'cargo build --manifest-path src-tauri/Cargo.toml',
    'tauri bundle',
    'x86_64-pc-windows-msvc',
    '--features win7-compat',
    'webview2-runtime',
    'Expand-Archive'
)) {
    if ($workflow -notlike "*$requiredText*") {
        throw "Win7 workflow is missing '$requiredText'."
    }
}

Write-Output 'Win7 WebView2 configuration is valid.'
