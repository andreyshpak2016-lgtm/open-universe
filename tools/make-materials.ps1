# Генератор материалов Open Universe для UE5.
#
# Создаёт /Game/Materials/M_BasePanel — параметрический материал построек базы
# (серо-металлическая поверхность с панельными линиями, параметр `Color`).
# Материал собирается из узлов, без внешних текстур, поэтому работает сразу
# после сборки.
#
# Использование:
#   powershell -ExecutionPolicy Bypass -File tools/make-materials.ps1
#   powershell -File tools/make-materials.ps1 -EngineRoot "D:\UE_5.5"

param(
  [string]$EngineRoot = ""
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$client = Join-Path $root 'client'
$uproject = Join-Path $client 'OpenUniverse.uproject'
$script = Join-Path $PSScriptRoot 'make-materials.py'

function Find-Engine
{
  $candidates = @()
  foreach ($base in @('C:\Program Files\Epic Games', 'D:\Epic Games', 'E:\Epic Games', 'D:\', 'E:\', 'C:\')) {
    if (Test-Path $base) {
      Get-ChildItem $base -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '^UE_5\.' } |
        ForEach-Object { $candidates += $_.FullName }
    }
  }
  $valid = @()
  foreach ($c in $candidates) {
    if (Test-Path (Join-Path $c 'Engine\Binaries\Win64\UnrealEditor-Cmd.exe')) { $valid += $c }
  }
  if ($valid.Count -eq 0) { return $null }
  $assoc = $null
  if (Test-Path $uproject) {
    $m = Select-String -Path $uproject -Pattern '"EngineAssociation"\s*:\s*"([^"]+)"'
    if ($m) { $assoc = $m.Matches[0].Groups[1].Value }
  }
  if ($assoc) {
    $match = @($valid | Where-Object { (Split-Path $_ -Leaf) -eq ("UE_" + $assoc) })
    if ($match.Count -gt 0) { return $match[0] }
  }
  return ($valid | Sort-Object { [version]($_ -replace '.*UE_','') } -Descending)[0]
}

if ($EngineRoot -and (Test-Path $EngineRoot)) { $engine = $EngineRoot } else { $engine = Find-Engine }
if (-not $engine) { throw "Unreal Engine не найден. Укажите -EngineRoot." }

$cmd = Join-Path $engine 'Engine\Binaries\Win64\UnrealEditor-Cmd.exe'
if (-not (Test-Path $cmd)) { throw "Не найден UnrealEditor-Cmd.exe: $cmd" }

$bundled = Get-ChildItem (Join-Path $engine 'Engine\Binaries\ThirdParty\DotNet') -Directory -ErrorAction SilentlyContinue |
  Sort-Object Name -Descending | Select-Object -First 1
if ($bundled) {
  $dn = Join-Path $bundled.FullName 'win-x64'
  if (Test-Path (Join-Path $dn 'dotnet.exe')) { $env:DOTNET_ROOT = $dn }
}

Write-Host "== Материалы Open Universe ==" -ForegroundColor Cyan
Write-Host "Движок: $engine"
Write-Host "Проект: $uproject"
Write-Host "Скрипт: $script"

& $cmd $uproject -run=pythonscript "-script=$script" -unattended -nosplash -nopause -stdout -FullStdOutLogOutput
if ($LASTEXITCODE -ne 0) { throw "Генерация материалов не удалась (код $LASTEXITCODE)" }

$mat = Join-Path $client 'Content\Materials\M_BasePanel.uasset'
if (Test-Path $mat) {
  Write-Host "Материал создан: $mat" -ForegroundColor Green
} else {
  Write-Host "Файл материала не найден по пути $mat — проверьте вывод." -ForegroundColor Yellow
}
