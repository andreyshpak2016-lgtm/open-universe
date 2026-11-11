# Запуск графического клиента Open Universe (Unreal Engine 5).
#
# Обычно вызывается через game.bat (двойной клик).
# Сервер запускайте отдельно — server.bat.
#
# Использование:
#   powershell -ExecutionPolicy Bypass -File tools/game.ps1
#   powershell -File tools/game.ps1 -EngineRoot "D:\UE_5.5" -Port 7777
#
# Что делает:
#   1. Находит движок (по EngineAssociation проекта).
#   2. Собирает C++-модуль, если он ещё не собран.
#   3. Создаёт карту, если её нет.
#   4. Открывает редактор. Нажмите Play (Alt+P) — корабль подключится к серверу.

param(
  [string]$EngineRoot = "",
  [int]$Port = 7777
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$client = Join-Path $root 'client'
$uproject = Join-Path $client 'OpenUniverse.uproject'
$map = Join-Path $client 'Content\Maps\OpenUniverse.umap'

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
  # Версия движка проекта (EngineAssociation), а не самая новая.
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

Write-Host "=========================================" -ForegroundColor Cyan
Write-Host " Open Universe - ИГРА (Unreal Engine 5)" -ForegroundColor Cyan
Write-Host "=========================================" -ForegroundColor Cyan

if (-not (Test-Path $uproject)) { throw "Не найден проект: $uproject" }

if ($EngineRoot -and (Test-Path $EngineRoot)) { $engine = $EngineRoot } else { $engine = Find-Engine }
if (-not $engine) {
  Write-Host "Unreal Engine не найден." -ForegroundColor Red
  Write-Host "Установите UE 5.5+ или укажите путь: game.bat -EngineRoot D:\UE_5.5" -ForegroundColor Yellow
  exit 1
}
Write-Host " Движок: $engine"
Write-Host " Проект: $uproject"
Write-Host ""

# --- Сборка модуля (если нужно) ---
$dll = Join-Path $client 'Binaries\Win64\UnrealEditor-OpenUniverse.dll'
if (-not (Test-Path $dll)) {
  Write-Host "--> модуль не собран, собираю (tools/build-client.ps1)" -ForegroundColor Yellow
  & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'build-client.ps1') -EngineRoot $engine -Build
  if ($LASTEXITCODE -ne 0) { throw "Сборка модуля не удалась" }
} else {
  Write-Host "Модуль уже собран: $dll" -ForegroundColor DarkGray
}

# --- Карта (если нет) ---
if (-not (Test-Path $map)) {
  Write-Host "--> карты нет, создаю (tools/make-map.ps1)" -ForegroundColor Yellow
  & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'make-map.ps1') -EngineRoot $engine
  if ($LASTEXITCODE -ne 0) { Write-Host "Не удалось создать карту — редактор всё равно открою." -ForegroundColor DarkYellow }
} else {
  Write-Host "Карта найдена: $map" -ForegroundColor DarkGray
}

# --- Запуск редактора ---
$editor = Join-Path $engine 'Engine\Binaries\Win64\UnrealEditor.exe'
Write-Host ""
Write-Host "--> открываю Unreal Editor" -ForegroundColor Yellow
Write-Host "    В редакторе нажмите Play (Alt+P)." -ForegroundColor Green
Write-Host "    Клиент подключится к серверу 127.0.0.1:$Port" -ForegroundColor Green
Write-Host "    (сервер запускается отдельно: server.bat)" -ForegroundColor DarkGray

Start-Process -FilePath $editor -ArgumentList "`"$uproject`"" -WorkingDirectory $client
