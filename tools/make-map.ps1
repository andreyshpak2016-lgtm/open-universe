# Генерация карты Open Universe для UE5-редактора.
#
# Без карты редактор открывает пустую сцену — получается чёрный экран.
# Скрипт создаёт /Game/Maps/OpenUniverse со светом, небом, туманом,
# точкой старта, воксельной планетой и менеджером начала координат.
#
# Использование:
#   powershell -ExecutionPolicy Bypass -File tools/make-map.ps1
#   powershell -File tools/make-map.ps1 -EngineRoot "D:\UE_5.5"

param(
  [string]$EngineRoot = ""
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$client = Join-Path $root 'client'
$uproject = Join-Path $client 'OpenUniverse.uproject'
$script = Join-Path $PSScriptRoot 'make-map.py'

function Find-Engine
{
  $candidates = @()
  foreach ($base in @('C:\Program Files\Epic Games', 'D:\Epic Games', 'E:\Epic Games', 'D:\', 'E:\', 'C:\'))
  {
    if (Test-Path $base)
    {
      Get-ChildItem $base -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '^UE_5\.' } |
        ForEach-Object { $candidates += $_.FullName }
    }
  }
  $valid = @()
  foreach ($c in $candidates)
  {
    if (Test-Path (Join-Path $c 'Engine\Binaries\Win64\UnrealEditor-Cmd.exe')) { $valid += $c }
  }
  if ($valid.Count -eq 0) { return $null }

  # Проект собирается под конкретную версию движка (EngineAssociation в .uproject).
  # Берём именно её, иначе UBT попытается пересобрать модуль под другой движок.
  $assoc = $null
  if (Test-Path $uproject)
  {
    $m = Select-String -Path $uproject -Pattern '"EngineAssociation"\s*:\s*"([^"]+)"'
    if ($m) { $assoc = $m.Matches[0].Groups[1].Value }
  }
  if ($assoc)
  {
    $match = @($valid | Where-Object { (Split-Path $_ -Leaf) -eq ("UE_" + $assoc) })
    if ($match.Count -gt 0) { return $match[0] }
  }

  # Иначе — самая новая версия.
  return ($valid | Sort-Object { [version]($_ -replace '.*UE_','') } -Descending)[0]
}

if ($EngineRoot -and (Test-Path $EngineRoot)) { $engine = $EngineRoot } else { $engine = Find-Engine }
if (-not $engine) { throw "Unreal Engine не найден. Укажите -EngineRoot." }

$cmd = Join-Path $engine 'Engine\Binaries\Win64\UnrealEditor-Cmd.exe'
if (-not (Test-Path $cmd)) { throw "Не найден UnrealEditor-Cmd.exe: $cmd" }

# .NET из движка — для UnrealBuildTool при подгрузке модуля.
$bundled = Get-ChildItem (Join-Path $engine 'Engine\Binaries\ThirdParty\DotNet') -Directory -ErrorAction SilentlyContinue |
  Sort-Object Name -Descending | Select-Object -First 1
if ($bundled) {
  $dn = Join-Path $bundled.FullName 'win-x64'
  if (Test-Path (Join-Path $dn 'dotnet.exe')) { $env:DOTNET_ROOT = $dn }
}

Write-Host "== Генерация карты Open Universe ==" -ForegroundColor Cyan
Write-Host "Движок: $engine"
Write-Host "Проект: $uproject"
Write-Host "Скрипт: $script"

& $cmd $uproject -run=pythonscript "-script=$script" -unattended -nosplash -nopause -stdout -FullStdOutLogOutput
if ($LASTEXITCODE -ne 0) { throw "Генерация карты не удалась (код $LASTEXITCODE)" }

$map = Join-Path $client 'Content\Maps\OpenUniverse.umap'
if (Test-Path $map)
{
  Write-Host "Карта создана: $map" -ForegroundColor Green
}
else
{
  Write-Host "Готово, но файл карты не найден по пути $map — проверьте вывод." -ForegroundColor Yellow
}
