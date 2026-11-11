# Запуск игрового сервера Open Universe (Rust).
#
# Обычно вызывается через server.bat (двойной клик).
#
# Использование:
#   powershell -ExecutionPolicy Bypass -File tools/server.ps1
#   powershell -File tools/server.ps1 -Port 7800 -Seconds 600
#   powershell -File tools/server.ps1 -World .\world
#
# Сервер работает в этом же окне: видно тики, вход игроков и автосохранения.
# Чтобы остановить — нажмите Ctrl+C или закройте окно.

param(
  [int]$Port = 7777,
  [int]$Seconds = 3600,
  [string]$World = "",
  [switch]$NoBuild
)

$ErrorActionPreference = 'Stop'

# Корень репозитория — от расположения скрипта, поэтому запускать можно откуда угодно.
$root = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path (Join-Path $root 'server\Cargo.toml'))) {
  Write-Host "Не найден репозиторий Open Universe рядом со скриптом:" -ForegroundColor Red
  Write-Host "  ожидался: $root\server\Cargo.toml"
  exit 1
}
$server = Join-Path $root 'server'
$exe = Join-Path $server 'target\release'

Write-Host "=========================================" -ForegroundColor Cyan
Write-Host " Open Universe - СЕРВЕР" -ForegroundColor Cyan
Write-Host "=========================================" -ForegroundColor Cyan
Write-Host " Адрес:   127.0.0.1:$Port"
Write-Host " Время:   $(if ($Seconds -ge 3600) { 'без ограничения (пока не закроете окно)' } else { "$Seconds с" })"
if ($World) { Write-Host " Мир:     $World (сохраняется между запусками)" }
Write-Host ""

# --- Сборка ---
$netd = Join-Path $exe 'ou-netd.exe'
if (-not $NoBuild) {
  Write-Host "--> сборка сервера (cargo build --release)" -ForegroundColor Yellow
  Push-Location $server
  try {
    & cargo build --release -p ou-server --bin ou-netd
    if ($LASTEXITCODE -ne 0) { throw "Сборка не удалась" }
  } finally {
    Pop-Location
  }
}
if (-not (Test-Path $netd)) { throw "Не найден $netd - соберите проект: cargo build --release" }

# --- Запуск ---
$args = @("127.0.0.1:$Port", "$Seconds")
if ($World) { $args += $World }

Write-Host ""
Write-Host "--> сервер запущен. Для игры откройте game.bat" -ForegroundColor Green
Write-Host "    Остановить: Ctrl+C" -ForegroundColor DarkGray
Write-Host ""

$env:RUST_BACKTRACE = "1"
Push-Location $server
try {
  & $netd @args
} finally {
  Pop-Location
}

Write-Host ""
Write-Host "Сервер остановлен." -ForegroundColor Yellow
