# Запуск Open Universe для проверки вручную (UE 5.5).
#
# Поднимает сервер ou-netd, открывает клиент и поднимает окно игры ПОВЕРХ ВСЕХ,
# чтобы можно было проверять фичи вживую. Запускать из корня репозитория или
# откуда угодно — путь определяется сам.
#
# Использование:
#   powershell -ExecutionPolicy Bypass -File tools/play-ue.ps1
#   powershell -File tools/play-ue.ps1 -EngineRoot "C:\Program Files\Epic Games\UE_5.5"
#   powershell -File tools/play-ue.ps1 -Name MyPilot -Seconds 7200
#
# Остановить: закройте окно игры; сервер можно закрыть `Stop-Process -Name ou-netd`.

param(
  [string]$EngineRoot = "",
  [string]$Name = "Pilot",
  [int]$Seconds = 14400,
  [int]$Port = 7777
)

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
$proj = Join-Path $root 'client\OpenUniverse.uproject'

function Find-Editor {
  if ($EngineRoot -and (Test-Path $EngineRoot)) {
    return (Join-Path $EngineRoot 'Engine\Binaries\Win64\UnrealEditor.exe')
  }
  foreach ($base in @('D:\UE_5.5', 'C:\Program Files\Epic Games\UE_5.5', 'D:\Epic Games\UE_5.5')) {
    $e = Join-Path $base 'Engine\Binaries\Win64\UnrealEditor.exe'
    if (Test-Path $e) { return $e }
  }
  $found = Get-ChildItem 'D:\', 'C:\', 'E:\' -Directory -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -match '^UE_5\.' } |
    Where-Object { Test-Path (Join-Path $_.FullName 'Engine\Binaries\Win64\UnrealEditor.exe') } |
    Select-Object -First 1 -ExpandProperty FullName
  if ($found) { return (Join-Path $found 'Engine\Binaries\Win64\UnrealEditor.exe') }
  return $null
}

$editor = Find-Editor
if (-not $editor -or -not (Test-Path $editor)) {
  Write-Host "Unreal Engine 5.5 не найден. Укажите -EngineRoot." -ForegroundColor Red
  exit 1
}

# Сервер (собираем, если исходники есть).
$netd = Join-Path $root 'server\target\debug\ou-netd.exe'
if (-not (Test-Path $netd)) { $netd = Join-Path $root 'server\target\release\ou-netd.exe' }
if (-not (Test-Path $netd) -and (Test-Path (Join-Path $root 'server\Cargo.toml'))) {
  Write-Host "--> сборка ou-netd" -ForegroundColor Yellow
  Push-Location (Join-Path $root 'server')
  try { cargo build -q -p ou-server --bin ou-netd } finally { Pop-Location }
}

Write-Host "--> сервер: $netd 127.0.0.1:$Port $Seconds" -ForegroundColor Yellow
Start-Process -FilePath $netd -ArgumentList @("127.0.0.1:$Port", "$Seconds") `
  -WorkingDirectory (Join-Path $root 'server') -WindowStyle Minimized
Start-Sleep -Seconds 3

Write-Host "--> клиент UE 5.5" -ForegroundColor Yellow
Start-Process -FilePath $editor `
  -ArgumentList "`"$proj`"", '-game', '-windowed', '-ResX=1600', '-ResY=900', '-OUAutoConnect', ("-OUName=" + $Name) `
  -WorkingDirectory (Join-Path $root 'client')

# Ждём окно и поднимаем его поверх всех (иначе останется за другими окнами).
Write-Host "ожидание окна игры..." -ForegroundColor DarkGray
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public class OUPlay {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr p);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  public delegate bool EnumProc(IntPtr h, IntPtr p);
}
"@
function Find-Game {
  $script:g = [IntPtr]::Zero
  $cb = [OUPlay+EnumProc]{
    param($h, $p)
    if (-not [OUPlay]::IsWindowVisible($h)) { return $true }
    $sb = New-Object System.Text.StringBuilder 512
    [void][OUPlay]::GetWindowText($h, $sb, 512)
    if ($sb.ToString() -match 'OpenUniverse \(') { $script:g = $h; return $false }
    return $true
  }
  [void][OUPlay]::EnumWindows($cb, [IntPtr]::Zero)
  return $script:g
}
for ($i = 0; $i -lt 120; $i++) {
  Start-Sleep -Seconds 2
  $h = Find-Game
  if ($h -ne [IntPtr]::Zero) {
    [void][OUPlay]::SetWindowPos($h, [IntPtr](-1), 60, 60, 0, 0, 0x1 -bor 0x2 -bor 0x40)
    [void][OUPlay]::SetForegroundWindow($h)
    Write-Host "окно игры поднято поверх всех" -ForegroundColor Green
    break
  }
}
Write-Host "Готово. Играйте. (Alt — освободить курсор; Alt+F4 — выход)" -ForegroundColor Green
