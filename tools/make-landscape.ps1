# Генерация ландшафта планеты: numpy -> C++-мост -> UE Landscape.
#
# ВАЖНО: ландшафт строится только в ПОЛНОЙ сессии редактора
# (UnrealEditor.exe -ExecutePythonScript=...). В commandlet-режиме движок
# падает на создании ландшафта, поэтому здесь именно UnrealEditor.exe.
#
# Использование:
#   powershell -ExecutionPolicy Bypass -File tools/make-landscape.ps1
#   powershell -File tools/make-landscape.ps1 -Size 4033 -EngineRoot "D:\UE_5.5"
#
# Окно НЕ открывается: редактор запускается с -RenderOffscreen. Проверка при
# этом идёт по данным (чтение высот обратно из ландшафта) + превью рельефа
# средствами системного Python (tools/landscape-preview.py).

param(
  [string]$EngineRoot = "",
  # Размер кратен 63 (квадов в компоненте) плюс 1: 505, 1009, 2017, 4033, 8065.
  [int]$Size = 2017,
  [switch]$KeepOpen
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$client = Join-Path $root 'client'
$uproject = Join-Path $client 'OpenUniverse.uproject'
$script = Join-Path $PSScriptRoot 'make-landscape.py'
$report = Join-Path $env:TEMP 'opencode\land_result.txt'
$stdout = Join-Path $env:TEMP 'opencode\land_stdout.txt'

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
    if (Test-Path (Join-Path $c 'Engine\Binaries\Win64\UnrealEditor.exe')) { $valid += $c }
  }
  if ($valid.Count -eq 0) { return $null }
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
  return ($valid | Sort-Object { [version]($_ -replace '.*UE_','') } -Descending)[0]
}

if ($EngineRoot -and (Test-Path $EngineRoot)) { $engine = $EngineRoot } else { $engine = Find-Engine }
if (-not $engine) { throw "Unreal Engine не найден. Укажите -EngineRoot." }

$editor = Join-Path $engine 'Engine\Binaries\Win64\UnrealEditor.exe'
if (-not (Test-Path $editor)) { throw "Не найден UnrealEditor.exe: $editor" }

Get-Process UnrealEditor,UnrealEditor-Cmd,CrashReportClient -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2
Remove-Item $report,$stdout -ErrorAction SilentlyContinue

$env:OU_LAND_SIZE = "$Size"
# Путь для ExecutePythonScript — с прямыми слэшами (UE так надёжнее).
$scriptArg = $script -replace '\\', '/'

Write-Host "== Ландшафт Open Universe ==" -ForegroundColor Cyan
Write-Host "Движок: $engine"
Write-Host "Размер: $Size x $Size вершин"

$args = @(
  $uproject,
  "-ExecutePythonScript=$scriptArg",
  '-RenderOffscreen',
  '-nosound',
  '-nosplash',
  '-stdout'
)
$p = Start-Process -FilePath $editor -ArgumentList $args -PassThru -NoNewWindow -RedirectStandardOutput $stdout

# Ждём маркер завершения (отчёт пишется инкрементально, поэтому просто
# существования файла недостаточно) — до 15 минут.
$waited = 0
$done = $false
while ($waited -lt 900) {
  if (Test-Path $report) {
    $txt = Get-Content $report -Raw -Encoding UTF8
    if ($txt -match 'ГОТОВО|ОШИБКА') { $done = $true; break }
  }
  Start-Sleep -Seconds 5
  $waited += 5
}

if (Test-Path $report) {
  Write-Host "`n--- отчёт ---" -ForegroundColor Yellow
  Get-Content $report -Encoding UTF8
} else {
  Write-Host "Отчёт не появился за $waited с." -ForegroundColor Red
  if (Test-Path $stdout) {
    Get-Content $stdout | Select-String 'Error|Assertion|Fatal' | Select-Object -Last 10
  }
}

if (-not $KeepOpen) {
  Get-Process UnrealEditor,UnrealEditor-Cmd,CrashReportClient -ErrorAction SilentlyContinue | Stop-Process -Force
}
