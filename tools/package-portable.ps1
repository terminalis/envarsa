# Packages the portable Windows build: envarsa.exe + WebView2Loader.dll
# zipped as envarsa_<version>_x64_portable.zip (version from tauri.conf.json).
# Run after `npm run build`; needs no toolchain, just Windows PowerShell.
$ErrorActionPreference = 'Stop'

$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$conf = Get-Content (Join-Path $root 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json
$version = $conf.version

# The release dir moves when cargo gets an explicit --target; if both exist,
# package whichever exe was built most recently.
$releaseDir = @(
    Join-Path $root 'src-tauri\target\release'
    Join-Path $root 'src-tauri\target\x86_64-pc-windows-gnu\release'
) |
    Where-Object { Test-Path (Join-Path $_ 'envarsa.exe') } |
    Sort-Object { (Get-Item (Join-Path $_ 'envarsa.exe')).LastWriteTime } -Descending |
    Select-Object -First 1
if (-not $releaseDir) {
    throw "envarsa.exe not found in any release dir - run 'npm run build' first."
}

$exe = Join-Path $releaseDir 'envarsa.exe'
$dll = Join-Path $releaseDir 'WebView2Loader.dll'
if (-not (Test-Path $dll)) {
    throw "WebView2Loader.dll missing next to $exe - the portable exe cannot run without it."
}

$outDir = Join-Path $releaseDir 'bundle\portable'
New-Item -ItemType Directory -Force $outDir | Out-Null
$zip = Join-Path $outDir "envarsa_${version}_x64_portable.zip"
Compress-Archive -LiteralPath $exe, $dll -DestinationPath $zip -Force

$mb = [math]::Round((Get-Item $zip).Length / 1MB, 2)
Write-Output "wrote $zip ($mb MB)"
