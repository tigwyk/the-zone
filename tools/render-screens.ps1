# Render the dumped store-page screens into PNGs.
#   1. cargo test -- --ignored dump_screens_for_release   # writes target/screens/*.grid
#   2. .\tools\render-screens.ps1                          # writes screenshots/*.png
#
# The grid dump carries each cell's char, sRGB colour and bold flag, so this draws
# the real screens with the real palette and the game's own "bold = brighter" rule
# (render.rs `bold_color`), plus the default CRT scanline overlay.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$srcDir = Join-Path $root "target\screens"
$outDir = Join-Path $root "screenshots"
New-Item -ItemType Directory -Path $outDir -Force | Out-Null

Add-Type -AssemblyName System.Drawing

# Cell aspect is the game's 0.6-em advance over 1.2-em line height (1:2).
$cellW = 12
$cellH = 24
$cols = 120
$rows = 33
$W = $cols * $cellW    # 1440
$H = $rows * $cellH    # 792

# Bevy's default clear colour, what the window shows through the blank cells.
$bg = [System.Drawing.Color]::FromArgb(26, 26, 26)

$font = New-Object System.Drawing.Font("Consolas", 20, [System.Drawing.GraphicsUnit]::Pixel)

function Brighten([System.Drawing.Color]$c, [int]$bold) {
    if ($bold) {
        return [System.Drawing.Color]::FromArgb(
            [int][math]::Round(0.4 * $c.R + 153),
            [int][math]::Round(0.4 * $c.G + 153),
            [int][math]::Round(0.4 * $c.B + 153))
    }
    return $c
}

function Render-Grid([string]$gridFile, [string]$outFile) {
    $grid = New-Object 'object[,]' $rows, $cols
    foreach ($line in [System.IO.File]::ReadLines($gridFile)) {
        $p = $line -split ' '
        $x = [int]$p[0]; $y = [int]$p[1]; $cp = [int]$p[2]
        if ($cp -ne 32) {  # blanks are the clear colour, not ink
            $grid[$y, $x] = @{
                ch = [char]$cp
                c  = Brighten ([System.Drawing.Color]::FromArgb([int]$p[3], [int]$p[4], [int]$p[5])) ([int]$p[6])
            }
        }
    }

    $bmp = New-Object System.Drawing.Bitmap $W, $H
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.Clear($bg)
    $g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
    $sf = New-Object System.Drawing.StringFormat
    $sf.Alignment = [System.Drawing.StringAlignment]::Near
    $sf.LineAlignment = [System.Drawing.StringAlignment]::Near
    $sf.FormatFlags = [System.Drawing.StringFormatFlags]::MeasureTrailingSpaces

    $brush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::Black)
    for ($y = 0; $y -lt $rows; $y++) {
        for ($x = 0; $x -lt $cols; $x++) {
            $cell = $grid[$y, $x]
            if ($cell) {
                $brush.Color = $cell.c
                $g.DrawString([string]$cell.ch, $font, $brush, ($x * $cellW), ($y * $cellH), $sf)
            }
        }
    }

    # The default CRT overlay: every third pixel row, black at alpha 30.
    $scan = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(30, 0, 0, 0))
    for ($y = 0; $y -lt $H; $y += 3) { $g.FillRectangle($scan, 0, $y, $W, 1) }

    $bmp.Save($outFile, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Write-Output "rendered $outFile ($W x $H)"
}

$grids = Get-ChildItem $srcDir -Filter *.grid | Sort-Object Name
if (-not $grids) { throw "no .grid files in $srcDir - run the dump test first" }
foreach ($f in $grids) {
    Render-Grid $f.FullName (Join-Path $outDir ($f.BaseName + ".png"))
}
Write-Output "done"
