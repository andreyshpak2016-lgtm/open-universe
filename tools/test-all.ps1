# Единый прогон всех проверок Open Universe.
#
# Запускает максимум доступных тестов на этой машине и честно сообщает, что
# пропущено и почему (нет Unreal Engine, нет упакованной сборки и т. п.).
# Удобно для второго компьютера: одна команда — весь отчёт.
#
# Использование:
#   powershell -ExecutionPolicy Bypass -File tools/test-all.ps1
#   powershell -File tools/test-all.ps1 -SkipUE        # без тяжёлых UE-тестов
#   powershell -File tools/test-all.ps1 -EngineRoot "D:\UE_5.5"
#
# Результат: сводная таблица «проверка → итог» и код возврата 0/1.

param(
  [switch]$SkipUE,
  [string]$EngineRoot = ""
)

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
$tools = $PSScriptRoot

function Find-UE {
  if ($EngineRoot -and (Test-Path $EngineRoot)) { return $EngineRoot }
  foreach ($base in @('D:\UE_5.5', 'C:\Program Files\Epic Games\UE_5.5', 'D:\Epic Games\UE_5.5')) {
    if (Test-Path (Join-Path $base 'Engine\Binaries\Win64\UnrealEditor.exe')) { return $base }
  }
  $found = Get-ChildItem 'D:\', 'C:\', 'E:\' -Directory -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -match '^UE_5\.' } |
    Where-Object { Test-Path (Join-Path $_.FullName 'Engine\Binaries\Win64\UnrealEditor.exe') } |
    Select-Object -First 1 -ExpandProperty FullName
  return $found
}

function Have-Cargo {
  return [bool](Get-Command cargo -ErrorAction SilentlyContinue)
}

function Have-MSVC {
  $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
  if (-not (Test-Path $vswhere)) { return $false }
  $vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
  return [bool]$vs
}

function Have-Packaged {
  $p = Join-Path $root 'client\Packaged\Windows\OpenUniverse\Binaries\Win64\OpenUniverse-Win64-Shipping.exe'
  $inst = Join-Path $env:LOCALAPPDATA 'Programs\Open Universe\OpenUniverse\Binaries\Win64\OpenUniverse-Win64-Shipping.exe'
  return (Test-Path $p) -or (Test-Path $inst)
}

$engine = Find-UE
$hasCargo = Have-Cargo
$hasMsvc = Have-MSVC
$hasUE = [bool]$engine
$hasPackaged = Have-Packaged

Write-Host "== Проверка Open Universe: единый прогон ==" -ForegroundColor Cyan
$ueInfo = if ($hasUE) { " ($engine)" } else { "" }
Write-Host ("окружение: Rust={0} MSVC={1} UE5={2}{3} упаковка={4}" -f $hasCargo, $hasMsvc, $hasUE, $ueInfo, $hasPackaged) -ForegroundColor DarkGray
Write-Host ""

$results = @()
function Run-Step {
  param([string]$Name, [string]$Script, [string[]]$ScriptArgs = @(), [bool]$Enabled = $true, [string]$SkipReason = "")
  if (-not $Enabled) {
    Write-Host ("--- {0}: ПРОПУЩЕНО ({1})" -f $Name, $SkipReason) -ForegroundColor DarkYellow
    $script:results += [pscustomobject]@{ Name = $Name; Ok = $null; Note = "пропущено: $SkipReason" }
    return
  }
  Write-Host ("--- {0}" -f $Name) -ForegroundColor Cyan
  $path = Join-Path $tools $Script
  if (-not (Test-Path $path)) {
    Write-Host ("  скрипт не найден: {0}" -f $path) -ForegroundColor Red
    $script:results += [pscustomobject]@{ Name = $Name; Ok = $false; Note = "нет скрипта" }
    return
  }
  & powershell -NoProfile -ExecutionPolicy Bypass -File $path @ScriptArgs
  $code = $LASTEXITCODE
  $ok = ($code -eq 0)
  $script:results += [pscustomobject]@{ Name = $Name; Ok = $ok; Note = ("код $code") }
  Write-Host ""
}

Run-Step -Name 'cargo test --workspace' -Script 'test-rust.ps1' -Enabled $hasCargo -SkipReason 'нет cargo (установите Rust)'
Run-Step -Name 'test-client (провод)' -Script 'test-client.ps1' -Enabled ($hasCargo -and $hasMsvc) -SkipReason 'нужны Rust и MSVC'
Run-Step -Name 'test-terrain (рельеф)' -Script 'test-terrain.ps1' -Enabled ($hasCargo -and $hasMsvc) -SkipReason 'нужны Rust и MSVC'

$ueArgs = @()
if ($EngineRoot) { $ueArgs += @('-EngineRoot', $EngineRoot) }
Run-Step -Name 'test-ue (редактор)' -Script 'test-ue.ps1' -ScriptArgs $ueArgs -Enabled ($hasUE -and -not $SkipUE) -SkipReason 'нет Unreal Engine 5.5 (или -SkipUE)'
Run-Step -Name 'test-shipping (упакованная)' -Script 'test-shipping.ps1' -Enabled ($hasPackaged -and -not $SkipUE) -SkipReason 'нет упакованной сборки (сначала package-game.ps1)'

Write-Host "== Итог ==" -ForegroundColor Cyan
$fail = 0
$pass = 0
$skip = 0
foreach ($r in $results) {
  if ($null -eq $r.Ok) { $skip++; $mark = '—'; $color = 'DarkYellow' }
  elseif ($r.Ok) { $pass++; $mark = 'OK'; $color = 'Green' }
  else { $fail++; $mark = 'FAIL'; $color = 'Red' }
  Write-Host ("  {0,-26} {1,-5} {2}" -f $r.Name, $mark, $r.Note) -ForegroundColor $color
}
Write-Host ""
Write-Host ("пройдено {0}, провалено {1}, пропущено {2}" -f $pass, $fail, $skip) -ForegroundColor Yellow

if ($fail -gt 0) { exit 1 }
exit 0
