Add-Type -AssemblyName System.Drawing
$bmp = New-Object System.Drawing.Bitmap 32, 32
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.Clear([System.Drawing.Color]::Transparent)

# Draw rounded rectangle for YouTube white screen
$path = New-Object System.Drawing.Drawing2D.GraphicsPath
$r = 5.0
$x = 2.0; $y = 6.0; $w = 28.0; $h = 20.0
$path.AddArc($x, $y, $r*2, $r*2, 180.0, 90.0)
$path.AddArc($x + $w - $r*2, $y, $r*2, $r*2, 270.0, 90.0)
$path.AddArc($x + $w - $r*2, $y + $h - $r*2, $r*2, $r*2, 0.0, 90.0)
$path.AddArc($x, $y + $h - $r*2, $r*2, $r*2, 90.0, 90.0)
$path.CloseFigure()

$brush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 255, 255, 255))
$g.FillPath($brush, $path)

# Cut out play triangle in center
$g.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
$clearBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(0, 0, 0, 0))
$triPath = New-Object System.Drawing.Drawing2D.GraphicsPath
$p1 = New-Object System.Drawing.PointF 13.0, 11.0
$p2 = New-Object System.Drawing.PointF 21.0, 16.0
$p3 = New-Object System.Drawing.PointF 13.0, 21.0
$triPath.AddPolygon(@($p1, $p2, $p3))
$g.FillPath($clearBrush, $triPath)

$g.Dispose()
$outPath = Join-Path $PSScriptRoot 'assets\tray_icon_white.png'
$bmp.Save($outPath, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Host "Saved to $outPath"
