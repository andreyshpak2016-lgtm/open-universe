# Запуск игрового сервера Open Universe.
#
# Обычно вызывается через server.bat (двойной клик).
# Работает и из репозитория (соберёт через cargo), и из установленной версии
# (использует готовые .exe рядом со скриптом).
#
# Использование:
#   powershell -ExecutionPolicy Bypass -File tools/server.ps1
#   powershell -File tools/server.ps1 -Port 7800 -Seconds 600
#   powershell -File tools/server.ps1 -World .\world

param(
  [int]$Port = 7777,
  [int]$Seconds = 3600,
  [string]$World = "",
  [switch]$NoBuild
)

$ErrorActionPreference = 'Stop'

# Общий помощник: определяем раскладку (репозиторий или установка).
$common = Join-Path $PSScriptRoot 'ou-common.ps1'
if (Test-Path $common) { . $common }
$layout = Resolve-OU -StartDir $PSScriptRoot

if (-not $layout) {
  Write-Host "Не найдены файлы Open Universe." -ForegroundColor Red
  Write-Host "  Ожидался ou-netd.exe рядом со скриптами или каталог server\\Cargo.toml."
  Read-Host "Нажмите Enter, чтобы закрыть"
  exit 1
}

$server = Join-Path $layout.Root 'server'
$exe = $layout.ExeDir
$netd = Join-Path $exe 'ou-netd.exe'

# Сборка — только если есть исходники и это не запрещено.
if ($layout.HasSources -and -not $NoBuild -and -not (Test-Path $netd)) {
  Write-Host "--> сборка сервера (cargo build --release)" -ForegroundColor Yellow
  Push-Location $server
  try {
    & cargo build --release -p ou-server --bin ou-netd
    if ($LASTEXITCODE -ne 0) { throw "Сборка не удалась" }
  } finally { Pop-Location }
}
if (-not (Test-Path $netd)) {
  Write-Host "Не найден ou-netd.exe: $netd" -ForegroundColor Red
  Read-Host "Нажмите Enter, чтобы закрыть"
  exit 1
}

Write-Host "=========================================" -ForegroundColor Cyan
Write-Host " Open Universe - СЕРВЕР" -ForegroundColor Cyan
Write-Host "=========================================" -ForegroundColor Cyan
Write-Host " Адрес:   127.0.0.1:$Port"
Write-Host " Время:   $(if ($Seconds -ge 3600) { 'без ограничения (пока не закроете окно)' } else { "$Seconds с" })"
if ($World) { Write-Host " Мир:     $World (сохраняется между запусками)" }
Write-Host ""
Write-Host "--> сервер запущен. Для игры откройте game.bat" -ForegroundColor Green
Write-Host "    Остановить: Ctrl+C" -ForegroundColor DarkGray
Write-Host ""

$cliArgs = @("127.0.0.1:$Port", "$Seconds")
# Каталог мира: свой или рядом с установкой, чтобы мир сохранялся.
$worldDir = if ($World) { $World } elseif ($layout.World) { $layout.World } else { "" }
if ($worldDir) { $cliArgs += $worldDir }

$env:RUST_BACKTRACE = "1"
Push-Location $exe
try {
  & $netd @cliArgs
} finally {
  Pop-Location
}

Write-Host ""
Write-Host "Сервер остановлен." -ForegroundColor Yellow
