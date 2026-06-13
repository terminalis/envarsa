# Generates the Envarsa Open Graph card (1200x630 PNG) with System.Drawing.
# Mirrors tools/make-icon.ps1 — same vault-dark palette and key-bar / masked-dots
# motif ("the mask IS the brand"). Left: the logo mark, "Envarsa" wordmark, the
# tagline and a trust line. Right: a masked .env store panel. The result is saved
# to brand/og-image.png and referenced by og:image / twitter:image in index.html.
Add-Type -AssemblyName System.Drawing

$W = 1200; $H = 630
$bmp = New-Object System.Drawing.Bitmap($W, $H)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::ClearTypeGridFit
$g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic

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

function C([int]$r, [int]$g, [int]$b) { [System.Drawing.Color]::FromArgb(255, $r, $g, $b) }

$bg       = C 15 17 22      # #0f1116  page
$plate    = C 19 21 27      # #13151b  mark / rail
$panel    = C 23 26 33      # #171a21  store panel
$border   = C 38 43 54      # #262b36  borders / frame
$rim      = C 44 50 63      # #2c323f  mark rim
$divider  = C 31 36 46      # #1f242e  soft divider
$text     = C 232 234 240   # #e8eaf0  wordmark
$muted    = C 139 147 167   # #8b93a7  tagline
$faint    = C 92 100 120    # #5c6478  panel header
$brass    = C 229 179 90    # #e5b35a  accent / keys
$brassDim = C 185 143 72    # #b98f48  trust line
$steel    = C 124 133 153   # #7c8599  mark dots
$mask     = C 107 115 136   # #6b7388  masked values

# Brushes
$bWord  = New-Object System.Drawing.SolidBrush($text)
$bTag   = New-Object System.Drawing.SolidBrush($muted)
$bTrust = New-Object System.Drawing.SolidBrush($brassDim)
$bFaint = New-Object System.Drawing.SolidBrush($faint)
$bKey   = New-Object System.Drawing.SolidBrush($brass)
$bMask  = New-Object System.Drawing.SolidBrush($mask)

# Baseline-accurate text: places the glyph baseline at $baselineY so positions
# match the approved mockup. Uses typographic metrics to map em pixels -> ascent.
$typo = [System.Drawing.StringFormat]::GenericTypographic
function DrawText([string]$txt, [string]$family, [float]$emPx, [System.Drawing.FontStyle]$style,
                  [System.Drawing.Brush]$brush, [float]$x, [float]$baselineY, [bool]$anchorEnd = $false) {
    $ff = New-Object System.Drawing.FontFamily($family)
    $font = New-Object System.Drawing.Font($ff, $emPx, $style, [System.Drawing.GraphicsUnit]::Pixel)
    $ascentPx = $emPx * $ff.GetCellAscent($style) / $ff.GetEmHeight($style)
    $tx = $x
    if ($anchorEnd) {
        $sz = $g.MeasureString($txt, $font, (New-Object System.Drawing.PointF(0, 0)), $typo)
        $tx = $x - $sz.Width
    }
    $g.DrawString($txt, $font, $brush, $tx, ($baselineY - $ascentPx), $typo)
    $font.Dispose(); $ff.Dispose()
}

# ---- canvas ----
$g.Clear($bg)
$frame = RoundedPath 32 32 1136 566 24
$g.DrawPath((New-Object System.Drawing.Pen($border, 1.5)), $frame)

# ---- logo mark (make-icon motif), ~150px at (96,150) ----
$mx = 96.0; $my = 150.0; $s = 150.0 / 128.0
function MX([float]$v) { $script:mx + $v * $script:s }
function MY([float]$v) { $script:my + $v * $script:s }

$markPlate = RoundedPath (MX 7.5) (MY 7.5) (113 * $s) (113 * $s) (27 * $s)
$g.FillPath((New-Object System.Drawing.SolidBrush($plate)), $markPlate)
$g.DrawPath((New-Object System.Drawing.Pen($rim, 1.5 * $s)), $markPlate)

$bar = New-Object System.Drawing.Pen($brass, 11 * $s)
$bar.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
$bar.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
$g.DrawLine($bar, (MX 35.5), (MY 44), (MX 71), (MY 44))
$dotBrush = New-Object System.Drawing.SolidBrush($steel)
$dr = 5.3 * $s
foreach ($cx in 35.3, 52.7, 70, 87.4) {
    $g.FillEllipse($dotBrush, (MX $cx) - $dr, (MY 63.5) - $dr, $dr * 2, $dr * 2)
}
$g.DrawLine($bar, (MX 35.5), (MY 83.5), (MX 92), (MY 83.5))

# ---- wordmark + tagline + trust line ----
DrawText "Envarsa" "Segoe UI" 104 ([System.Drawing.FontStyle]::Bold) $bWord 96 392
DrawText "a local-first library for"   "Segoe UI" 30 ([System.Drawing.FontStyle]::Regular) $bTag 100 448
DrawText "your environment variables"  "Segoe UI" 30 ([System.Drawing.FontStyle]::Regular) $bTag 100 488

$mid = [char]0x00B7
DrawText "no cloud $mid no telemetry $mid no egress by default" "Consolas" 22 ([System.Drawing.FontStyle]::Regular) $bTrust 98 552

# ---- store panel ----
$g.FillPath((New-Object System.Drawing.SolidBrush($panel)), (RoundedPath 724 150 380 300 18))
$g.DrawPath((New-Object System.Drawing.Pen($border, 1.5)), (RoundedPath 724 150 380 300 18))
DrawText "envarsa.store" "Consolas" 17 ([System.Drawing.FontStyle]::Regular) $bFaint 748 190
$g.DrawLine((New-Object System.Drawing.Pen($divider, 1)), 748, 206, 1080, 206)

$dot = [char]0x2022
$rows = @(
    @{ k = "DATABASE_URL"; n = 12 },
    @{ k = "API_KEY";      n = 8  },
    @{ k = "SECRET_TOKEN"; n = 14 },
    @{ k = "STRIPE_KEY";   n = 6  }
)
$y = 254
foreach ($row in $rows) {
    DrawText $row.k "Consolas" 21 ([System.Drawing.FontStyle]::Regular) $bKey 748 $y
    DrawText ([string]$dot * $row.n) "Consolas" 21 ([System.Drawing.FontStyle]::Regular) $bMask 930 $y
    $y += 54
}

# ---- url ----
DrawText "envarsa.dev" "Consolas" 23 ([System.Drawing.FontStyle]::Regular) $bKey 1080 552 $true

$g.Dispose()
$out = Join-Path $PSScriptRoot "..\brand\og-image.png"
$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Output ("wrote {0} ({1}x{2})" -f $out, $W, $H)
