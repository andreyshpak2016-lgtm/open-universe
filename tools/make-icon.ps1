# Draws the Open Universe app icon and packs a multi-size .ico.
# ASCII-only on purpose (avoids PowerShell parser issues with non-ASCII).
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$outDir = 'D:\Pro\open-universe\assets'
$icoPath = Join-Path $outDir 'openuniverse.ico'
$pngPath = Join-Path $outDir 'openuniverse-256.png'

function New-IconBitmap([int]$size) {
  $bmp = New-Object System.Drawing.Bitmap($size, $size)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
  $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
  $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality

  $s = [double]$size / 256.0
  $f = { param($v) return [float]($v * $s) }

  # Background: rounded square, dark navy radial gradient.
  $r = & $f 58
  $rect = New-Object System.Drawing.RectangleF(0, 0, $size, $size)
  $path = New-Object System.Drawing.Drawing2D.GraphicsPath
  $d = $r * 2
  $path.AddArc(0, 0, $d, $d, 180, 90)
  $path.AddArc($size - $d, 0, $d, $d, 270, 90)
  $path.AddArc($size - $d, $size - $d, $d, $d, 0, 90)
  $path.AddArc(0, $size - $d, $d, $d, 90, 90)
  $path.CloseFigure()

  $bgBrush = New-Object System.Drawing.Drawing2D.PathGradientBrush($path)
  $bgBrush.CenterColor = [System.Drawing.Color]::FromArgb(255, 16, 22, 48)
  $bgBrush.SurroundColors = @([System.Drawing.Color]::FromArgb(255, 5, 6, 15))
  $bgBrush.CenterPoint = New-Object System.Drawing.PointF([float]($size*0.5), [float]($size*0.42))
  $g.FillPath($bgBrush, $path)

  # Stars.
  $rand = New-Object System.Random(7)
  for ($i = 0; $i -lt 26; $i++) {
    $x = $rand.NextDouble() * $size
    $y = $rand.NextDouble() * $size
    # keep stars inside the rounded square
    $dx = [Math]::Abs($x - $size/2); $dy = [Math]::Abs($y - $size/2)
    if ($dx -gt ($size*0.46) -or $dy -gt ($size*0.46)) { continue }
    $sr = [float](($rand.NextDouble() * 0.9 + 0.4) * $s)
    $op = [int](90 + $rand.NextDouble() * 150)
    $sb = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb($op, 255, 255, 255))
    $g.FillEllipse($sb, [float]$x, [float]$y, $sr, $sr)
    $sb.Dispose()
  }

  # Accretion disk: rotated ellipse ring with blue->cyan gradient.
  $cx = [float]($size * 0.5); $cy = [float]($size * 0.5)
  $state = $g.Save()
  $g.TranslateTransform($cx, $cy)
  $g.RotateTransform(-18)

  $rx = [float]($size * 0.40); $ry = [float]($size * 0.155)
  $diskRect = New-Object System.Drawing.RectangleF((-1*$rx), (-1*$ry), ($rx*2), ($ry*2))
  $diskPath = New-Object System.Drawing.Drawing2D.GraphicsPath
  $diskPath.AddEllipse($diskRect)
  $lg = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
    (New-Object System.Drawing.PointF((-1*$rx),0)),
    (New-Object System.Drawing.PointF($rx,0)),
    [System.Drawing.Color]::FromArgb(255, 106, 139, 255),
    [System.Drawing.Color]::FromArgb(255, 0, 230, 208))
  $penWidth = [float]($size * 0.075)
  $pen = New-Object System.Drawing.Pen($lg, $penWidth)
  $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
  $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
  $glowPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(60, 100, 160, 255), [float]($penWidth * 1.55))
  $glowPen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
  $glowPen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round

  # Back half of the disk (drawn first, gets partly hidden by the core).
  $g.DrawPath($glowPen, $diskPath)
  $g.DrawPath($pen, $diskPath)

  $g.Restore($state)

  # Black hole core (event horizon) with a soft rim.
  $rr = [float]($size * 0.180)
  $rim = New-Object System.Drawing.Drawing2D.GraphicsPath
  $rim.AddEllipse($cx - $rr*1.20, $cy - $rr*1.20, $rr*2.40, $rr*2.40)
  $rimPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(150, 120, 170, 255), [float]($rr*0.34))
  $g.DrawPath($rimPen, $rim)

  $coreBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 0, 0, 2))
  $g.FillEllipse($coreBrush, $cx - $rr, $cy - $rr, $rr*2, $rr*2)
  $edgePen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(230, 150, 200, 255), [float]($rr*0.12))
  $g.DrawEllipse($edgePen, $cx - $rr, $cy - $rr, $rr*2, $rr*2)

  # Front half of the disk, drawn on top so it wraps in front of the core.
  $state2 = $g.Save()
  $g.TranslateTransform($cx, $cy)
  $g.RotateTransform(-18)
  $g.DrawArc($glowPen, $diskRect, 8, 164)
  $g.DrawArc($pen, $diskRect, 8, 164)
  $hi = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(240, 255, 255, 255), [float]($penWidth * 0.30))
  $g.DrawArc($hi, $diskRect, 118, 52)
  $g.Restore($state2)

  # Subtle outer border of the tile.
  $brd = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(60, 130, 170, 255), [float](1.4 * $s))
  $g.DrawPath($brd, $path)

  $g.Dispose()
  $bgBrush.Dispose(); $path.Dispose(); $pen.Dispose(); $glowPen.Dispose(); $hi.Dispose()
  $rimPen.Dispose(); $coreBrush.Dispose(); $edgePen.Dispose(); $brd.Dispose()
  $diskPath.Dispose(); $lg.Dispose()
  return $bmp
}

# Render sizes.
$sizes = @(16, 24, 32, 48, 64, 128, 256)
$pngs = @()
foreach ($sz in $sizes) {
  $b = New-IconBitmap $sz
  $ms = New-Object System.IO.MemoryStream
  $b.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
  $pngs += ,($ms.ToArray())
  if ($sz -eq 256) { $b.Save($pngPath, [System.Drawing.Imaging.ImageFormat]::Png) }
  $b.Dispose(); $ms.Dispose()
}
Write-Host ("Rendered {0} sizes; 256px -> {1}" -f $sizes.Count, $pngPath)

# Pack ICO (Vista+ PNG-compressed entries).
$fs = [System.IO.File]::Create($icoPath)
$bw = New-Object System.IO.BinaryWriter($fs)
$bw.Write([UInt16]0)          # reserved
$bw.Write([UInt16]1)          # type = icon
$bw.Write([UInt16]$sizes.Count)
$offset = 6 + (16 * $sizes.Count)
for ($i = 0; $i -lt $sizes.Count; $i++) {
  $sz = $sizes[$i]
  $data = $pngs[$i]
  $dim = $sz
  if ($sz -ge 256) { $dim = 0 }
  $bw.Write([Byte]$dim)     # width
  $bw.Write([Byte]$dim)     # height
  $bw.Write([Byte]0)        # colors
  $bw.Write([Byte]0)        # reserved
  $bw.Write([UInt16]1)      # planes
  $bw.Write([UInt16]32)     # bpp
  $bw.Write([UInt32]$data.Length)
  $bw.Write([UInt32]$offset)
  $offset += $data.Length
}
foreach ($data in $pngs) { $bw.Write($data) }
$bw.Flush(); $bw.Close(); $fs.Close()

Write-Host ("ICO: {0} ({1:N0} bytes)" -f $icoPath, (Get-Item $icoPath).Length)
