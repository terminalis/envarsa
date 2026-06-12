# Generates the Envarsa brand icon (1024x1024 PNG) with System.Drawing.
# Motif: a vault-dark rounded square holding an env list — a visible key
# bar, a masked value (dots), and another key bar. The mask IS the brand.
Add-Type -AssemblyName System.Drawing

$size = 1024
$bmp = New-Object System.Drawing.Bitmap($size, $size)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.Clear([System.Drawing.Color]::Transparent)

function RoundedPath([float]$x, [float]$y, [float]$w, [float]$h, [float]$r) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = $r * 2
    $p.AddArc($x, $y, $d, $d, 180, 90)
    $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
    $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $p.CloseFigure()
    return $p
}

$bg     = [System.Drawing.Color]::FromArgb(255, 19, 21, 27)    # vault dark
$edge   = [System.Drawing.Color]::FromArgb(255, 44, 50, 63)    # subtle rim
$amber  = [System.Drawing.Color]::FromArgb(255, 229, 179, 90)  # brass accent
$muted  = [System.Drawing.Color]::FromArgb(255, 124, 133, 153) # steel

# Plate
$plate = RoundedPath 64 64 896 896 200
$g.FillPath((New-Object System.Drawing.SolidBrush($bg)), $plate)
$pen = New-Object System.Drawing.Pen($edge, 14)
$g.DrawPath($pen, $plate)

# Row 1: a key bar (visible label)
$g.FillPath((New-Object System.Drawing.SolidBrush($amber)), (RoundedPath 240 312 380 86 43))
# Row 2: the masked value — four dots
$dotBrush = New-Object System.Drawing.SolidBrush($muted)
$y = 469; $r = 43
foreach ($cx in 283, 423, 563, 703) {
    $g.FillEllipse($dotBrush, $cx - $r, $y, $r * 2, $r * 2)
}
# Row 3: another key bar, longer (the list continues)
$g.FillPath((New-Object System.Drawing.SolidBrush($amber)), (RoundedPath 240 626 544 86 43))

$g.Dispose()
$out = Join-Path $PSScriptRoot "..\brand\icon-1024.png"
New-Item -ItemType Directory -Force (Split-Path $out) | Out-Null
$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Output "wrote $out"
