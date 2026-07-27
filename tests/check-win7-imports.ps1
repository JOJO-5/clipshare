param(
    [Parameter(Mandatory = $true)]
    [string]$BinaryPath,

    [string]$DumpbinPath
)

$resolvedBinary = (Resolve-Path -LiteralPath $BinaryPath).Path
if ([string]::IsNullOrWhiteSpace($DumpbinPath)) {
    $dumpbinExecutable = (Get-Command dumpbin.exe -ErrorAction Stop).Source
} else {
    $dumpbinExecutable = (Resolve-Path -LiteralPath $DumpbinPath).Path
}
$imports = & $dumpbinExecutable /nologo /imports $resolvedBinary
if ($LASTEXITCODE -ne 0) {
    throw "dumpbin failed for $resolvedBinary"
}

$forbiddenImports = @(
    'EventSetInformation',
    'GetSystemTimePreciseAsFileTime',
    'GetPackagesByPackageFamily'
)
$forbiddenLibraries = @(
    'combase.dll'
)

$violations = @()
foreach ($name in $forbiddenImports) {
    if ($imports -match "(?im)\b$([regex]::Escape($name))\b") {
        $violations += $name
    }
}
foreach ($library in $forbiddenLibraries) {
    if ($imports -match "(?im)^\s*$([regex]::Escape($library))\s*$") {
        $violations += $library
    }
}

if ($violations.Count -gt 0) {
    throw "Win7-incompatible imports found in $resolvedBinary`: $($violations -join ', ')"
}

Write-Output "Win7 import check passed: $resolvedBinary"
