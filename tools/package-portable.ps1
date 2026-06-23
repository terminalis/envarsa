# Packages the portable Windows build: envarsa.exe + WebView2Loader.dll +
# an envarsa.portable marker, zipped as envarsa_<version>_x64_portable.zip
# (version from tauri.conf.json). The marker makes the unzipped folder
# self-contained: with it beside the exe, Envarsa keeps config.json and the
# store file in that folder instead of %APPDATA%.
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

# The marker that flips the app into portable mode. Its presence beside the
# exe is what matters; the text is just a note for anyone who finds it.
$marker = Join-Path $outDir 'envarsa.portable'
Set-Content -LiteralPath $marker -Encoding UTF8 -Value @'
This file marks Envarsa as a portable install.
While it sits beside envarsa.exe, Envarsa keeps its config.json and store
file in this folder rather than %APPDATA%, so the whole folder travels as
one unit. Delete it to fall back to the per-user %APPDATA% location.
'@

$zip = Join-Path $outDir "envarsa_${version}_x64_portable.zip"
Compress-Archive -LiteralPath $exe, $dll, $marker -DestinationPath $zip -Force
Remove-Item -LiteralPath $marker -Force

$mb = [math]::Round((Get-Item $zip).Length / 1MB, 2)
Write-Output "wrote $zip ($mb MB)"
