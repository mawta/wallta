$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$iconDir = Join-Path $PSScriptRoot '../src-tauri/icons'
New-Item -ItemType Directory -Force -Path $iconDir | Out-Null
foreach ($size in @(32, 128, 256)) {
    $bitmap = New-Object System.Drawing.Bitmap($size, $size)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $graphics.Clear([System.Drawing.Color]::FromArgb(24, 31, 32))
    $brush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(225, 235, 182))
    $points = [System.Drawing.PointF[]]@(
        [System.Drawing.PointF]::new($size * .15, $size * .27),
        [System.Drawing.PointF]::new($size * .3, $size * .27),
        [System.Drawing.PointF]::new($size * .39, $size * .61),
        [System.Drawing.PointF]::new($size * .48, $size * .4),
        [System.Drawing.PointF]::new($size * .59, $size * .61),
        [System.Drawing.PointF]::new($size * .69, $size * .27),
        [System.Drawing.PointF]::new($size * .85, $size * .27),
        [System.Drawing.PointF]::new($size * .67, $size * .76),
        [System.Drawing.PointF]::new($size * .52, $size * .76),
        [System.Drawing.PointF]::new($size * .48, $size * .64),
        [System.Drawing.PointF]::new($size * .44, $size * .76),
        [System.Drawing.PointF]::new($size * .29, $size * .76))
    $graphics.FillPolygon($brush, $points)
    $bitmap.Save((Join-Path $iconDir "$size.png"), [System.Drawing.Imaging.ImageFormat]::Png)
    $graphics.Dispose(); $brush.Dispose(); $bitmap.Dispose()
}
$png = [System.IO.File]::ReadAllBytes((Join-Path $iconDir '256.png'))
$stream = New-Object System.IO.MemoryStream
$writer = New-Object System.IO.BinaryWriter($stream)
$writer.Write([uint16]0); $writer.Write([uint16]1); $writer.Write([uint16]1)
$writer.Write([byte]0); $writer.Write([byte]0); $writer.Write([byte]0); $writer.Write([byte]0)
$writer.Write([uint16]1); $writer.Write([uint16]32); $writer.Write([uint32]$png.Length); $writer.Write([uint32]22)
$writer.Write($png)
[System.IO.File]::WriteAllBytes((Join-Path $iconDir 'icon.ico'), $stream.ToArray())
$writer.Dispose(); $stream.Dispose()
