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

$installerHooks = $config.bundle.windows.nsis.installerHooks
if ([string]::IsNullOrWhiteSpace($installerHooks)) {
    throw 'Win7 NSIS installer must configure an installerHooks file.'
}
$hooksPath = Join-Path (Split-Path -Parent $configPath) $installerHooks
if (-not (Test-Path -LiteralPath $hooksPath -PathType Leaf)) {
    throw "Win7 NSIS installer hooks file was not found: '$hooksPath'."
}

$hooks = Get-Content -LiteralPath $hooksPath -Raw
foreach ($requiredHookText in @(
    'NSIS_HOOK_PREINSTALL',
    'CheckIfAppIsRunning "clipshare.exe" "ClipShare"',
    '$INSTDIR\webview2-runtime\msedge_elf.dll',
    'MB_RETRYCANCEL',
    'IDRETRY',
    'Abort'
)) {
    if (-not $hooks.Contains($requiredHookText)) {
        throw "Win7 installer hooks are missing '$requiredHookText'."
    }
}
if ($hooks -match '(?i)taskkill.+msedgewebview2\.exe') {
    throw 'Win7 installer hooks must not terminate every WebView2 process on the machine.'
}

$workflow = Get-Content -LiteralPath $workflowPath -Raw
$win7BuildStart = $workflow.IndexOf('- name: Build (Win7)')
$win7BuildEnd = $workflow.IndexOf('- name: Build (Windows)', $win7BuildStart)
if ($win7BuildStart -lt 0 -or $win7BuildEnd -lt 0) {
    throw 'Win7 build workflow section was not found.'
}
$win7Build = $workflow.Substring($win7BuildStart, $win7BuildEnd - $win7BuildStart)
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

$unsafeWildcardCopy = "Copy-Item -LiteralPath (Join-Path `$webviewExecutable.Directory.FullName '*')"
if ($workflow.Contains($unsafeWildcardCopy)) {
    throw 'Win7 workflow must not pass a wildcard through Copy-Item -LiteralPath.'
}
if (-not $workflow.Contains('Get-ChildItem -LiteralPath $webviewExecutable.Directory.FullName -Force | ForEach-Object')) {
    throw 'Win7 workflow must enumerate the fixed runtime directory before copying its contents.'
}
if ($win7Build.IndexOf('npm run build') -lt 0 -or $win7Build.IndexOf('npm run build') -gt $win7Build.IndexOf('cargo build')) {
    throw 'Win7 workflow must build the frontend before the raw Cargo build.'
}

Write-Output 'Win7 WebView2 configuration is valid.'
