# Самопроверка «сервер + клиент» Open Universe.
#
# Запускает ou-health: он поднимает реальный UDP-сервер, подключает клиент,
# проверяет симуляцию и спавн на планете, и пишет status.json для сайта и
# лаунчера. Возвращает код 1, если что-то не работает.
#
# Использование:
#   powershell -ExecutionPolicy Bypass -File tools/health.ps1
#   powershell -ExecutionPolicy Bypass -File tools/health.ps1 -Out status.json

param(
  [string]$Out = ""
)

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
$server = Join-Path $root 'server'
if (-not $Out) { $Out = Join-Path $root 'status.json' }

Write-Host "== Проверка сервера и клиента Open Universe ==" -ForegroundColor Cyan

Push-Location $server
try {
  cargo run -q -p ou-server --bin ou-health -- "$Out"
  $code = $LASTEXITCODE
} finally {
  Pop-Location
}

if ($code -ne 0) {
  Write-Host "Проверка НЕ пройдена (код $code). Смотрите $Out" -ForegroundColor Red
  exit $code
}

if (Test-Path $Out) {
  $j = Get-Content $Out -Raw | ConvertFrom-Json
  $state = if ($j.ok) { 'OK' } else { 'FAIL' }
  Write-Host ("Статус: {0} | сервер {1} | клиент {2} | симуляция {3} | спавн {4}" -f `
    $state, $j.server, $j.client, $j.simulation, $j.spawnOnPlanet) -ForegroundColor Green
}
exit 0
