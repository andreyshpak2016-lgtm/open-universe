# Автотест УПАКОВАННОЙ (Shipping) сборки игры Open Universe.
#
# Правило проекта: упакованную Shipping-сборку проверяем ВСЕГДА — именно её
# получает игрок и запускает лаунчер.
#
# Особенность Shipping: движок вырезает логи и offscreen-скриншоты
# (`UE_BUILD_SHIPPING`), поэтому проверить «по логу», как в tools/test-ue.ps1
# (редакторная сборка), нельзя. Здесь иначе:
#   1. запускаем РЕАЛЬНОЕ окно игры (не offscreen);
#   2. ждём, ставим окно в известную позицию и копируем кадр через GDI;
#   3. проверяем, что окно живо и что кадр не пустой/не чёрный.
#
# Путь к сборке задаётся параметром -GameDir; по умолчанию — установленная
# версия (`%LOCALAPPDATA%\Programs\Open Universe`), иначе — client\Packaged\Windows.
#
# Использование:
#   powershell -ExecutionPolicy Bypass -File tools/test-shipping.ps1
#   powershell -ExecutionPolicy Bypass -File tools/test-shipping.ps1 -GameDir "D:\...\Packaged\Windows"
# Результат: shots/shipping.png и код возврата 0/1.

param(
  [int]$WaitSec = 55,
  [string]$Pilot = "TestPilot",
  [string]$GameDir = "",
  [string]$Flag = "-OUBaseOverview",
  [switch]$KeepOpen
)

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
$shots = Join-Path $root 'shots'
New-Item -ItemType Directory -Force -Path $shots | Out-Null

# --- GDI-обёртка для снятия кадра с окна ---
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class OUship {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
Add-Type -AssemblyName System.Drawing

function Resolve-GameExe {
  param([string]$Dir)
  $candidates = @()
  if ($Dir) { $candidates += $Dir }
  # Установленная версия (её запускает лаунчер).
  $candidates += (Join-Path $env:LOCALAPPDATA 'Programs\Open Universe')
  # Упакованная сборка в репозитории.
  $candidates += (Join-Path $root 'client\Packaged\Windows')
  foreach ($d in $candidates) {
    if (-not $d -or -not (Test-Path $d)) { continue }
    $ship = Join-Path $d 'OpenUniverse\Binaries\Win64\OpenUniverse-Win64-Shipping.exe'
    if (Test-Path $ship) { return $ship }
  }
  return $null
}

$exe = Resolve-GameExe -Dir $GameDir
if (-not $exe) {
  Write-Host "Shipping exe not found (OpenUniverse-Win64-Shipping.exe)" -ForegroundColor Red
  exit 1
}
$workDir = Split-Path $exe

# Сервер (из репозитория или установки).
$serverExe = Join-Path $root 'server\target\debug\ou-netd.exe'
if (-not (Test-Path $serverExe)) { $serverExe = Join-Path $root 'server\target\release\ou-netd.exe' }
if (-not (Test-Path $serverExe)) { $serverExe = Join-Path $env:LOCALAPPDATA 'Programs\Open Universe\server\ou-netd.exe' }

function Stop-ShipProcs {
  for ($i = 0; $i -lt 5; $i++) {
    $procs = Get-Process OpenUniverse,OpenUniverse-Win64-Shipping,CrashReportClient,ou-netd -ErrorAction SilentlyContinue
    if (-not $procs) { break }
    $procs | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 800
  }
}
Stop-ShipProcs
trap { Stop-ShipProcs; break }

Write-Host "== Shipping build test ==" -ForegroundColor Cyan
Write-Host "game: $exe"

$srv = $null
if (Test-Path $serverExe) {
  $worldDir = Join-Path $env:TEMP 'ou-ship-test-world'
  $srv = Start-Process -FilePath $serverExe -ArgumentList '127.0.0.1:7777','120',$worldDir -PassThru -WindowStyle Hidden
  Write-Host "server PID $($srv.Id) on 127.0.0.1:7777"
  Start-Sleep -Seconds 3
} else {
  Write-Host "ou-netd.exe not found - test without server" -ForegroundColor DarkYellow
}

[void][OUship]::SetCursorPos(2, 2)
$cmdArgs = "-windowed -ResX=1280 -ResY=800 -nosound -OUAutoConnect -OUName=$Pilot $Flag"
$p = Start-Process -FilePath $exe -ArgumentList $cmdArgs -PassThru -WorkingDirectory $workDir
Write-Host "client PID $($p.Id), waiting $WaitSec s"
Start-Sleep -Seconds $WaitSec
[void][OUship]::SetCursorPos(2, 2)

$p.Refresh()
$exited = $p.HasExited
if ($exited) { Write-Host "client exited early (code $($p.ExitCode))" -ForegroundColor Red }

$ok = $true
function Check([string]$label, [bool]$cond) {
  if ($cond) { Write-Host ("  [OK] " + $label) -ForegroundColor Green }
  else { Write-Host ("  [FAIL] " + $label) -ForegroundColor Red; $script:ok = $false }
}

$shotPath = Join-Path $shots 'shipping.png'
$captured = $false
$h = $p.MainWindowHandle
if ($h -ne 0) {
  [void][OUship]::ShowWindow($h, 9)
  [void][OUship]::SetWindowPos($h, [IntPtr](-1), 40, 40, 0, 0, 0x0001 -bor 0x0010)
  [void][OUship]::SetForegroundWindow($h)
  Start-Sleep -Seconds 3
  $r = New-Object OUship+RECT
  [void][OUship]::GetWindowRect($h, [ref]$r)
  $w = $r.Right - $r.Left; $ht = $r.Bottom - $r.Top
  if ($w -gt 8 -and $ht -gt 8) {
    $bmp = New-Object System.Drawing.Bitmap($w, $ht)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($r.Left, $r.Top, 0, 0, (New-Object System.Drawing.Size($w, $ht)))
    $bmp.Save($shotPath, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    $captured = $true
    Write-Host "screenshot: $shotPath ($w x $ht)" -ForegroundColor Green
  }
}

# Кадр не должен быть пустым/чёрным: считаем долю «непустых» пикселей.
$nonBlank = 0.0
if ($captured -and (Test-Path $shotPath)) {
  $img = [System.Drawing.Bitmap]::FromFile($shotPath)
  $stepX = [Math]::Max(1, [int]($img.Width / 40)); $stepY = [Math]::Max(1, [int]($img.Height / 40))
  $total = 0; $lit = 0
  for ($y = 0; $y -lt $img.Height; $y += $stepY) {
    for ($x = 0; $x -lt $img.Width; $x += $stepX) {
      $c = $img.GetPixel($x, $y); $total++
      if (($c.R + $c.G + $c.B) -gt 45) { $lit++ }
    }
  }
  $img.Dispose()
  if ($total -gt 0) { $nonBlank = [double]$lit / $total }
  Write-Host ("frame lit ratio: {0:P1}" -f $nonBlank)
}

if (-not $KeepOpen -and -not $exited) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
if ($srv) { Stop-Process -Id $srv.Id -Force -ErrorAction SilentlyContinue }

Write-Host "checks:"
Check "game window opened" ($h -ne 0)
Check "still running (no early crash)" (-not $exited)
Check "screenshot captured" $captured
Check "frame not blank/black (>2% lit)" ($nonBlank -gt 0.02)

Stop-ShipProcs

if ($ok) { Write-Host "Shipping build: OK" -ForegroundColor Green; exit 0 }
Write-Host "Shipping build: problems found" -ForegroundColor Red
exit 1
