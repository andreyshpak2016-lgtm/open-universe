# Builds launcher/banner.jpg: a wide, darkened hero strip for the launcher header.
# ASCII-only.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$src = 'D:\Pro\open-universe\assets\space-nebula.jpg'
$outDir = 'D:\Pro\open-universe\launcher'
$out = Join-Path $outDir 'banner.jpg'

$W = 1040; $H = 320

$img = [System.Drawing.Image]::FromFile($src)
$bmp = New-Object System.Drawing.Bitmap($W, $H)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality

# Cover-crop the source into the banner aspect.
$srcAspect = $img.Width / $img.Height
$dstAspect = $W / $H
if ($srcAspect -gt $dstAspect) {
  $ch = $img.Height
  $cw = [int]($ch * $dstAspect)
  $cx = [int](($img.Width - $cw) / 2)
  $cy = 0
} else {
  $cw = $img.Width
  $ch = [int]($cw / $dstAspect)
  $cx = 0
  $cy = [int](($img.Height - $ch) / 3)
}
$srcRect = New-Object System.Drawing.Rectangle($cx, $cy, $cw, $ch)
$dstRect = New-Object System.Drawing.Rectangle(0, 0, $W, $H)
$g.DrawImage($img, $dstRect, $srcRect, [System.Drawing.GraphicsUnit]::Pixel)

# Darken overall a touch, then a strong bottom-to-top gradient for text legibility.
$dark = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(70, 6, 8, 20))
$g.FillRectangle($dark, $dstRect)

$gradRect = New-Object System.Drawing.Rectangle(0, [int]($H*0.35), $W, [int]($H*0.65))
$grad = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
  $gradRect,
  [System.Drawing.Color]::FromArgb(0, 8, 10, 26),
  [System.Drawing.Color]::FromArgb(255, 8, 10, 26),
  [System.Drawing.Drawing2D.LinearGradientMode]::Vertical)
$g.FillRectangle($grad, $gradRect)

# Left-to-right vignette so the logo area stays readable.
$gradL = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
  (New-Object System.Drawing.Rectangle(0,0,[int]($W*0.7),$H)),
  [System.Drawing.Color]::FromArgb(120, 6, 8, 22),
  [System.Drawing.Color]::FromArgb(0, 6, 8, 22),
  [System.Drawing.Drawing2D.LinearGradientMode]::Horizontal)
$g.FillRectangle($gradL, (New-Object System.Drawing.Rectangle(0,0,[int]($W*0.7),$H)))

$g.Dispose()
$img.Dispose()

# Save as JPEG.
$enc = [System.Drawing.Imaging.ImageCodecInfo]::GetImageEncoders() | Where-Object { $_.MimeType -eq 'image/jpeg' }
$ep = New-Object System.Drawing.Imaging.EncoderParameters(1)
$ep.Param[0] = New-Object System.Drawing.Imaging.EncoderParameter([System.Drawing.Imaging.Encoder]::Quality, [int64]88)
$bmp.Save($out, $enc, $ep)
$bmp.Dispose()

Write-Host ("banner: {0}x{1}, {2:N0} bytes -> {3}" -f $W, $H, (Get-Item $out).Length, $out)
