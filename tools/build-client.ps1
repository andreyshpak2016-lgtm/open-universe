# Сборка UE5-клиента Open Universe.
#
# Использование:
#   powershell -ExecutionPolicy Bypass -File tools/build-client.ps1
#   powershell -File tools/build-client.ps1 -EngineRoot "D:\UE_5.5"
#   powershell -File tools/build-client.ps1 -Generate     # только файлы проекта
#   powershell -File tools/build-client.ps1 -Build        # только сборка
#
# Скрипт находит движок, генерирует файлы проекта и собирает цель Editor.
# Требуется: Unreal Engine 5.5+ и Visual Studio 2022 с C++.

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
  foreach ($base in @('C:\Program Files\Epic Games', 'D:\Epic Games', 'E:\Epic Games',
                      'D:\', 'E:\', 'C:\'))
  {
    if (Test-Path $base)
    {
      Get-ChildItem $base -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '^UE_5\.' } |
        ForEach-Object { $candidates += $_.FullName }
    }
  }
  foreach ($k in Get-ChildItem 'HKLM:\SOFTWARE\EpicGames\Unreal Engine' -ErrorAction SilentlyContinue)
  {
    $dir = (Get-ItemProperty $k.PSPath).InstalledDirectory
    if ($dir) { $candidates += $dir }
  }

  # Нужен Build.bat: по нему понимаем, что движок полный, а не заглушка.
  $valid = @()
  foreach ($c in $candidates)
  {
    $buildBat = Join-Path $c 'Engine\Build\BatchFiles\Build.bat'
    if (Test-Path $buildBat) { $valid += [PSCustomObject]@{ Path = $c; Build = $buildBat } }
  }
  if ($valid.Count -eq 0) { return $null }

  # Проект собирается под конкретную версию движка (EngineAssociation в .uproject).
  $assoc = $null
  if (Test-Path $uproject)
  {
    $m = Select-String -Path $uproject -Pattern '"EngineAssociation"\s*:\s*"([^"]+)"'
    if ($m) { $assoc = $m.Matches[0].Groups[1].Value }
  }
  if ($assoc)
  {
    $match = $valid | Where-Object { (Split-Path $_.Path -Leaf) -eq ("UE_" + $assoc) }
    if ($match) { return $match[0] }
  }

  # Иначе выбираем самую новую версию (по имени папки).
  return ($valid | Sort-Object { [version]($_.Path -replace '.*UE_','') } -Descending)[0]
}

Write-Host "== Сборка клиента Open Universe ==" -ForegroundColor Cyan

if (-not (Test-Path $uproject))
{
  throw "Не найден файл проекта: $uproject"
}

if ($EngineRoot -and (Test-Path $EngineRoot))
{
  $engine = [PSCustomObject]@{
    Path = $EngineRoot
    Build = (Join-Path $EngineRoot 'Engine\Build\BatchFiles\Build.bat')
  }
}
else
{
  $engine = Find-Engine
}
if (-not $engine -or -not (Test-Path $engine.Build))
{
  Write-Host "Unreal Engine не найден." -ForegroundColor Red
  Write-Host "Установите UE 5.5+ и повторите:" -ForegroundColor Yellow
  Write-Host '  powershell -File tools/build-client.ps1 -EngineRoot "D:\UE_5.5"' -ForegroundColor Yellow
  exit 1
}
Write-Host "Движок: $($engine.Path)" -ForegroundColor Green

# Дочерним процессам (UnrealBuildTool) нужен .NET. Используем тот, что идёт с UE.
$bundledDotnet = Get-ChildItem (Join-Path $engine.Path 'Engine\Binaries\ThirdParty\DotNet') -Directory -ErrorAction SilentlyContinue |
  Sort-Object Name -Descending | Select-Object -First 1
if ($bundledDotnet)
{
  $dn = Join-Path $bundledDotnet.FullName 'win-x64'
  if (Test-Path (Join-Path $dn 'dotnet.exe'))
  {
    $env:DOTNET_ROOT = $dn
    Write-Host ".NET из движка: $dn" -ForegroundColor DarkGray
  }
}

if (-not $Generate -and -not $Build) { $Generate = $true; $Build = $true }

# --- Генерация файлов проекта (для IDE и корректных путей) ---
if ($Generate)
{
  $genBat = Join-Path $engine.Path 'Engine\Build\BatchFiles\GenerateProjectFiles.bat'
  if (Test-Path $genBat)
  {
    Write-Host "--> генерация файлов проекта" -ForegroundColor Yellow
    & $genBat -project="$uproject" -game
  }
  else
  {
    Write-Host "GenerateProjectFiles.bat не найден — пропускаю (UBT соберёт и так)" -ForegroundColor DarkYellow
  }
}

# --- Сборка ---
if ($Build)
{
  Write-Host "--> сборка цели OpenUniverseEditor ($Config)" -ForegroundColor Yellow
  & $engine.Build OpenUniverseEditor Win64 $Config -Project="$uproject" -WaitMutex
  if ($LASTEXITCODE -ne 0) { throw "Сборка не удалась (код $LASTEXITCODE)" }

  $dll = Join-Path $client "Binaries\Win64\UnrealEditor-OpenUniverse.dll"
  if (Test-Path $dll)
  {
    Write-Host "Модуль собран: $dll" -ForegroundColor Green
  }
  Write-Host "Сборка завершена успешно" -ForegroundColor Green
}

$editor = Join-Path $engine.Path 'Engine\Binaries\Win64\UnrealEditor.exe'
Write-Host ""
Write-Host "Открыть редактор:" -ForegroundColor Cyan
Write-Host "  & `"$editor`" `"$uproject`""
