# Сборка UE5-клиента Open Universe.
#
# Использование:
#   powershell -ExecutionPolicy Bypass -File tools/build-client.ps1
#   powershell -ExecutionPolicy Bypass -File tools/build-client.ps1 -EngineRoot "D:\UE_5.5"
#
# Скрипт находит движок, генерирует файлы проекта и собирает цель Editor.
# Требуется: установленный Unreal Engine 5.5+ и Visual Studio 2022 с C++.

param(
  [string]$EngineRoot = "",
  [string]$Config = "Development",
  [switch]$Generate,
  [switch]$Build
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$client = Join-Path $root 'client'
$uproject = Join-Path $client 'OpenUniverse.uproject'

function Find-Engine
{
  # Ищем движок по стандартным местам и по реестру.
  $candidates = @()
  foreach ($base in @('C:\Program Files\Epic Games', 'D:\Epic Games', 'E:\Epic Games', 'D:\UnrealEngine', 'C:\UnrealEngine'))
  {
    if (Test-Path $base)
    {
      Get-ChildItem $base -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '^UE_' } |
        ForEach-Object { $candidates += $_.FullName }
    }
  }
  foreach ($k in Get-ChildItem 'HKLM:\SOFTWARE\EpicGames\Unreal Engine' -ErrorAction SilentlyContinue)
  {
    $dir = (Get-ItemProperty $k.PSPath).InstalledDirectory
    if ($dir) { $candidates += $dir }
  }
  foreach ($c in $candidates)
  {
    $ubt = Join-Path $c 'Engine\Binaries\DotNET\UnrealBuildTool\UnrealBuildTool.exe'
    if (Test-Path $ubt) { return $c }
  }
  return $null
}

Write-Host "== Сборка клиента Open Universe ==" -ForegroundColor Cyan

if (-not (Test-Path $uproject))
{
  throw "Не найден файл проекта: $uproject"
}

if ($EngineRoot -and (Test-Path $EngineRoot))
{
  $engine = $EngineRoot
}
else
{
  $engine = Find-Engine
}
if (-not $engine)
{
  Write-Host "Unreal Engine не найден." -ForegroundColor Red
  Write-Host "Установите UE 5.5+ через Epic Games Launcher и повторите:" -ForegroundColor Yellow
  Write-Host "  powershell -File tools/build-client.ps1 -EngineRoot ""D:\Epic Games\UE_5.5""" -ForegroundColor Yellow
  exit 1
}
Write-Host "Движок: $engine" -ForegroundColor Green

$ubt = Join-Path $engine 'Engine\Binaries\DotNET\UnrealBuildTool\UnrealBuildTool.exe'
$genScript = Join-Path $engine 'Engine\Build\BatchFiles\GenerateProjectFiles.bat'

# По умолчанию: сгенерировать проект и собрать.
if (-not $Generate -and -not $Build) { $Generate = $true; $Build = $true }

if ($Generate)
{
  if (-not (Test-Path $genScript))
  {
    Write-Host "Не найден GenerateProjectFiles.bat — генерирую через UBT" -ForegroundColor Yellow
    & $ubt -projectfiles -project="$uproject" -game -rocket -progress
  }
  else
  {
    & $genScript -project="$uproject" -game -engine
  }
  Write-Host "Файлы проекта сгенерированы" -ForegroundColor Green
}

if ($Build)
{
  Write-Host "--> сборка цели OpenUniverseEditor ($Config)" -ForegroundColor Yellow
  & $ubt OpenUniverseEditor Win64 $Config -Project="$uproject" -WaitMutex -FromMsBuild
  if ($LASTEXITCODE -ne 0) { throw "Сборка не удалась (код $LASTEXITCODE)" }
  Write-Host "Сборка завершена успешно" -ForegroundColor Green
}

Write-Host ""
Write-Host "Готово. Открыть редактор:" -ForegroundColor Cyan
Write-Host "  & ""$(Join-Path $engine 'Engine\Binaries\Win64\UnrealEditor.exe')"" ""$uproject"""
