# Проверка лаунчера Open Universe БЕЗ окна.
#
# Запускает встроенный самотест лаунчера (`--selfcheck`): он читает манифест,
# сверяет файлы установленной игры и сравнивает «Установлено» с «Доступно».
# С `-Repair` дополнительно чинит отставший VERSION — ту самую причину, из-за
# которой лаунчер показывал «Установлено: vX / Доступно: vY», хотя файлы были
# уже актуальны.
#
# Использование:
#   powershell -ExecutionPolicy Bypass -File tools/check-launcher.ps1
#   powershell -File tools/check-launcher.ps1 -Repair
#
# Окно не открывается. Код возврата 0 — версии совпали.

param(
  [switch]$Repair
)

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $root 'launcher\OpenUniverseLauncher.exe'

if (-not (Test-Path $exe)) {
  Write-Host "Лаунчер не собран: $exe" -ForegroundColor Red
  Write-Host "Соберите: powershell -File tools/build-launcher.ps1" -ForegroundColor Yellow
  exit 2
}

Write-Host "== Проверка лаунчера (версии) ==" -ForegroundColor Cyan
$args = @('--selfcheck')
if ($Repair) { $args += '--repair' }

& $exe $args
$code = $LASTEXITCODE
Write-Host ""
if ($code -eq 0) {
  Write-Host "Лаунчер: версии совпадают." -ForegroundColor Green
} else {
  Write-Host "Лаунчер: версии НЕ совпадают (смотрите вывод выше)." -ForegroundColor Yellow
}
exit $code
