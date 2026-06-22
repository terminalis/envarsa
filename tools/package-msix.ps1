# Builds an UNSIGNED MSIX for Microsoft Store submission from the current
# release build. The Store re-signs the package with a Microsoft certificate
# on ingestion, so this package must NOT be Authenticode-signed.
#
# Prereqs: release binary built (npm run build, or cargo build --release) and
# real Assets present (tools\generate-msix-assets.ps1). Windows SDK provides
# makeappx.exe.
#
#   pwsh tools\package-msix.ps1
#
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$rel = Join-Path $root 'src-tauri\target\release'
$manifest = Join-Path $root 'Package.appxmanifest'
$arch = 'x64'

$ver = ([xml](Get-Content $manifest -Raw)).Package.Identity.Version
if ($ver -like '0.*') { throw "Manifest version $ver has major 0, which the Store rejects. Bump to >= 1.0.0.0." }

# Locate the newest x64 makeappx.exe from the installed Windows SDK.
$makeappx = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin' -Recurse -Filter makeappx.exe -ErrorAction SilentlyContinue |
  Where-Object { $_.FullName -match '\\x64\\' } | Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
if (-not $makeappx) { throw 'makeappx.exe not found. Install the Windows SDK (App Cert Kit / SDK).' }

foreach ($f in @((Join-Path $rel 'envarsa.exe'), (Join-Path $rel 'WebView2Loader.dll'))) {
  if (-not (Test-Path $f)) { throw "Missing build artifact: $f (run the release build first)." }
}

# Guard against shipping placeholder tiles (the scaffold PNGs are tiny).
$store = Get-Item (Join-Path $root 'Assets\StoreLogo.png') -ErrorAction SilentlyContinue
if (-not $store -or $store.Length -lt 600) {
  Write-Warning ('Assets\StoreLogo.png is missing or looks like a placeholder ({0} bytes). Run tools\generate-msix-assets.ps1 first.' -f $store.Length)
}

# Assemble a clean payload (do not reuse dist\ to avoid stale files).
$payload = Join-Path $env:TEMP 'envarsa-msix-payload'
if (Test-Path $payload) { Remove-Item $payload -Recurse -Force }
New-Item -ItemType Directory -Force $payload | Out-Null
Copy-Item (Join-Path $rel 'envarsa.exe') $payload
Copy-Item (Join-Path $rel 'WebView2Loader.dll') $payload
Copy-Item (Join-Path $root 'Assets') (Join-Path $payload 'Assets') -Recurse
Copy-Item $manifest (Join-Path $payload 'AppxManifest.xml')

$outDir = Join-Path $root 'out'
New-Item -ItemType Directory -Force $outDir | Out-Null
$out = Join-Path $outDir ('Envarsa_{0}_{1}.msix' -f $ver, $arch)
if (Test-Path $out) { Remove-Item $out -Force }

& $makeappx pack /o /d $payload /p $out
if ($LASTEXITCODE -ne 0) { throw "makeappx failed (exit $LASTEXITCODE)" }

$mb = [math]::Round((Get-Item $out).Length / 1MB, 2)
('Built {0} ({1} MB), UNSIGNED. Submit to Partner Center; the Store signs it. Run WACK before submitting.' -f $out, $mb)
