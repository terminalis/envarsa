# Regenerates the MSIX Store tile/logo assets in Assets\ from the real app icon.
# Run whenever the branding changes so the package never ships placeholder art
# (the Store hard-rejects default scaffold tiles even though local WACK passes them).
#
#   pwsh tools\generate-msix-assets.ps1
#
param(
  [string]$Source = (Join-Path (Split-Path $PSScriptRoot -Parent) 'brand\icon-1024.png'),
  [string]$OutDir = (Join-Path (Split-Path $PSScriptRoot -Parent) 'Assets')
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$src = [System.Drawing.Image]::FromFile((Resolve-Path $Source).Path)
New-Item -ItemType Directory -Force $OutDir | Out-Null

function Save-Square([string]$name, [int]$size) {
  $bmp = New-Object System.Drawing.Bitmap($size, $size)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
  $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
  $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
  $g.Clear([System.Drawing.Color]::Transparent)
  $g.DrawImage($src, 0, 0, $size, $size)
  $g.Dispose()
  $bmp.Save((Join-Path $OutDir $name), [System.Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose()
}

function Save-Wide([string]$name, [int]$w, [int]$h) {
  # Square logo centred on a transparent wide canvas (height-fit).
  $bmp = New-Object System.Drawing.Bitmap($w, $h)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
  $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
  $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
  $g.Clear([System.Drawing.Color]::Transparent)
  $x = [int](($w - $h) / 2)
  $g.DrawImage($src, $x, 0, $h, $h)
  $g.Dispose()
  $bmp.Save((Join-Path $OutDir $name), [System.Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose()
}

# Names + sizes match the manifest references and winapp's scale qualifiers.
Save-Square 'StoreLogo.png'                              50
Save-Square 'MedTile.png'                                150
Save-Square 'MedTile.scale-200.png'                      300
Save-Square 'AppList.png'                                44
Save-Square 'AppList.scale-200.png'                      88
Save-Square 'AppList.targetsize-24_altform-unplated.png' 24
Save-Wide   'WideTile.png'                               310 150
Save-Wide   'WideTile.scale-200.png'                     620 300

$src.Dispose()
"Regenerated 8 MSIX assets in $OutDir from $Source"
