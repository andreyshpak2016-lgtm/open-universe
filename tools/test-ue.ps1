# Автотест UE-клиента Open Universe.
#
# Правило проекта (AGENTS.md): перед публикацией обязательно проверяем UE-клиент
# запуском и скриншотом. Скрипт:
#   1. поднимает Rust-сервер (ou-netd);
#   2. запускает игру с авто-подключением (-OUAutoConnect) и пилотом (-OUName);
#   3. ждёт, делает скриншот в shots/;
#   4. читает лог и проверяет ключевые признаки.
#
# Использование:
#   powershell -ExecutionPolicy Bypass -File tools/test-ue.ps1
# Результат: shots/ue.png и код возврата 0/1.

param(
  [int]$WaitSec = 45,
  [string]$Pilot = "TestPilot",
  [switch]$KeepOpen
)

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
$client = Join-Path $root 'client'
$proj = Join-Path $client 'OpenUniverse.uproject'
$shots = Join-Path $root 'shots'
$log = Join-Path $client 'Saved\Logs\OpenUniverse.log'

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class OUt {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
Add-Type -AssemblyName System.Drawing

function Find-Engine {
  foreach ($base in @('D:\UE_5.5','C:\Program Files\Epic Games\UE_5.5','D:\Epic Games\UE_5.5')) {
    if (Test-Path (Join-Path $base 'Engine\Binaries\Win64\UnrealEditor.exe')) { return $base }
  }
  Get-ChildItem 'D:\','C:\','E:\' -Directory -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -match '^UE_5\.' } |
    Where-Object { Test-Path (Join-Path $_.FullName 'Engine\Binaries\Win64\UnrealEditor.exe') } |
    Select-Object -First 1 -ExpandProperty FullName
}

$engine = Find-Engine
if (-not $engine) { Write-Host "UE not found" -ForegroundColor Red; exit 1 }
$editor = Join-Path $engine 'Engine\Binaries\Win64\UnrealEditor.exe'

$serverExe = Join-Path $root 'server\target\debug\ou-netd.exe'
if (-not (Test-Path $serverExe)) { $serverExe = Join-Path $root 'server\target\release\ou-netd.exe' }

# Гарантированная очистка: закрываем все процессы, что запускает тест, даже при
# ошибке или прерывании (Ctrl+C). Иначе после тестов остаются висеть игры.
function Stop-TestProcs {
  # Повторяем, пока процессы не исчезнут: редактор порождает дочерний игровой
  # процесс, который может пережить первый проход.
  for ($i = 0; $i -lt 5; $i++) {
    $procs = Get-Process UnrealEditor,OpenUniverse,UnrealEditor-Cmd,CrashReportClient,ou-netd -ErrorAction SilentlyContinue
    if (-not $procs) { break }
    $procs | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 800
  }
}
# На старте подчищаем хвосты прошлых запусков.
Stop-TestProcs
trap { Stop-TestProcs; break }

New-Item -ItemType Directory -Force -Path $shots | Out-Null
Remove-Item $log -ErrorAction SilentlyContinue
[void][OUt]::SetCursorPos(2, 2)

Write-Host "== UE client test ==" -ForegroundColor Cyan

$srv = $null
if (Test-Path $serverExe) {
  $worldDir = Join-Path $env:TEMP 'ou-ue-test-world'
  $srv = Start-Process -FilePath $serverExe -ArgumentList '127.0.0.1:7777','120',$worldDir -PassThru -WindowStyle Hidden
  Write-Host "server PID $($srv.Id) on 127.0.0.1:7777"
  Start-Sleep -Seconds 3
} else {
  Write-Host "ou-netd.exe not found - test without server" -ForegroundColor DarkYellow
}

$p = Start-Process -FilePath $editor `
  -ArgumentList "`"$proj`"",'-game','-windowed','-ResX=1280','-ResY=800','-OUAutoConnect','-OUAutoWalk','-OUAutoFire',("-OUName=" + $Pilot) -PassThru
Write-Host "client PID $($p.Id), waiting $WaitSec s"
Start-Sleep -Seconds $WaitSec
[void][OUt]::SetCursorPos(2, 2)

$p.Refresh()
if ($p.HasExited) { Write-Host "client exited early (code $($p.ExitCode))" -ForegroundColor Red }
$h = $p.MainWindowHandle
if ($h -ne 0) {
  [void][OUt]::ShowWindow($h, 9)
  [void][OUt]::SetWindowPos($h, [IntPtr](-1), 40, 40, 0, 0, 0x0001 -bor 0x0010)
  [void][OUt]::SetForegroundWindow($h)
  Start-Sleep -Seconds 2
  $r = New-Object OUt+RECT
  [void][OUt]::GetWindowRect($h, [ref]$r)
  $w = $r.Right - $r.Left; $ht = $r.Bottom - $r.Top
  if ($w -gt 8 -and $ht -gt 8) {
    $bmp = New-Object System.Drawing.Bitmap($w, $ht)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($r.Left, $r.Top, 0, 0, (New-Object System.Drawing.Size($w, $ht)))
    $bmp.Save((Join-Path $shots 'ue.png'), [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Write-Host "screenshot: $shots\ue.png ($w x $ht)" -ForegroundColor Green

    # Публикуем свежий скриншот в assets/: сайт (report.html) показывает именно
    # этот файл, поэтому отчёт всегда с актуальным кадром из движка.
    $assets = Join-Path $root 'assets'
    if (Test-Path $assets) {
      Copy-Item (Join-Path $shots 'ue.png') (Join-Path $assets 'ue-game.png') -Force
      Write-Host "опубликовано в assets/ue-game.png" -ForegroundColor Green
    }
  }
} else { Write-Host "no game window" -ForegroundColor Red }

if (-not $KeepOpen) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
if ($srv) { Stop-Process -Id $srv.Id -Force -ErrorAction SilentlyContinue }

# Финальная уборка: закрываем всё, что осталось от теста (в т.ч. CrashReportClient).
Stop-TestProcs

# --- проверки по логу ---
$ok = $true
function Check([string]$label, [bool]$cond) {
  if ($cond) { Write-Host ("  [OK] " + $label) -ForegroundColor Green }
  else { Write-Host ("  [FAIL] " + $label) -ForegroundColor Red; $script:ok = $false }
}
Write-Host "checks:"
if (Test-Path $log) {
  $text = Get-Content $log -Raw -Encoding UTF8
  Check "map loaded" ($text -match 'LoadMap\(/Game/Maps/OpenUniverse')
  Check "GameMode active" ($text -match 'OUGameMode|Game class is .OUGameMode')
  Check "planet built" ($text -match 'LogOUPlanet')
  Check "base built" ($text -match 'LogOUBase')
  Check "menu drawn" ($text -match 'LogOUMenu')
  Check "auto-connect reached server" ($text -match 'welcome|приветствие|подключено|Лог:.*\d') 
} else {
  Check "log exists" $false
}

if ($ok) { Write-Host "UE client: OK" -ForegroundColor Green; exit 0 }
Write-Host "UE client: problems found (see $log)" -ForegroundColor Red
exit 1
