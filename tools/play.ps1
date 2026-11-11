# Запуск Open Universe: сервер и клиент(ы).
#
# Использование:
#   powershell -ExecutionPolicy Bypass -File tools/play.ps1
#   powershell -File tools/play.ps1 -Players 3 -Seconds 30
#   powershell -File tools/play.ps1 -Port 7800 -World .\world
#
# Что произойдёт:
#   1. Соберётся release-версия (если нужно).
#   2. Запустится сервер единого шарда (ou-netd) на указанном порту.
#   3. Подключатся N клиентов (ou-netclient) — каждый со своим кораблём.
#   4. По окончании будет показана статистика: тики, трафик, позиции.

param(
  [int]$Port = 7777,
  [int]$Players = 2,
  [double]$Seconds = 20,
  [string]$World = "",
  [switch]$NoBuild
)

$ErrorActionPreference = 'Stop'

# Общий помощник: раскладка (репозиторий или установка).
$common = Join-Path $PSScriptRoot 'ou-common.ps1'
if (Test-Path $common) { . $common }
$layout = Resolve-OU -StartDir $PSScriptRoot
if (-not $layout) {
  Write-Host "Не найдены файлы Open Universe (ou-netd.exe)." -ForegroundColor Red
  Read-Host "Нажмите Enter, чтобы закрыть"
  exit 1
}
$server = Join-Path $layout.Root 'server'
$exe = $layout.ExeDir

Write-Host "=========================================" -ForegroundColor Cyan
Write-Host " Open Universe — запуск" -ForegroundColor Cyan
Write-Host "=========================================" -ForegroundColor Cyan
Write-Host " Сервер:  127.0.0.1:$Port"
Write-Host " Игроков: $Players"
Write-Host " Время:   $Seconds с"
if ($World) { Write-Host " Мир:     $World (сохраняется между запусками)" }
Write-Host ""

# --- 1. Сборка (только если есть исходники и exe ещё нет) ---
$netd = Join-Path $exe 'ou-netd.exe'
$netclient = Join-Path $exe 'ou-netclient.exe'
if ($layout.HasSources -and -not $NoBuild -and -not (Test-Path $netd)) {
  Write-Host "--> сборка (cargo build --release)" -ForegroundColor Yellow
  Push-Location $server
  try {
    & cargo build --release --workspace 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Сборка не удалась" }
  } finally {
    Pop-Location
  }
}

if (-not (Test-Path $netd)) { throw "Не найден $netd" }
if (-not (Test-Path $netclient)) { throw "Не найден $netclient" }

# --- 2. Сервер ---
$srvOut = Join-Path $env:TEMP 'ou-play-server.out'
$srvErr = Join-Path $env:TEMP 'ou-play-server.err'
$srvArgs = @("127.0.0.1:$Port", "$([int]$Seconds + 2)")
$worldDir = if ($World) { $World } elseif ($layout.World) { $layout.World } else { "" }
if ($worldDir) { $srvArgs += $worldDir }

Write-Host "--> старт сервера (ou-netd)" -ForegroundColor Yellow
$srv = Start-Process -FilePath $netd -ArgumentList $srvArgs `
  -RedirectStandardOutput $srvOut -RedirectStandardError $srvErr -NoNewWindow -PassThru
Start-Sleep -Milliseconds 800

# --- 3. Клиенты ---
Write-Host "--> подключение $Players игроков" -ForegroundColor Yellow
$clients = @()
for ($i = 1; $i -le $Players; $i++) {
  $out = Join-Path $env:TEMP "ou-play-client$i.out"
  $err = Join-Path $env:TEMP "ou-play-client$i.err"
  $cli = Start-Process -FilePath $netclient -ArgumentList "127.0.0.1:$Port", "$([int]$Seconds)" `
    -RedirectStandardOutput $out -RedirectStandardError $err -NoNewWindow -PassThru
  $clients += @{ Proc = $cli; Out = $out }
  Start-Sleep -Milliseconds 250
  Write-Host "    игрок $i подключился"
}

# --- 4. Ожидание ---
Write-Host "`n--> игра идёт $Seconds секунд..." -ForegroundColor Green
foreach ($c in $clients) { $c.Proc.WaitForExit(60000) | Out-Null }
$srv.WaitForExit(60000) | Out-Null

# --- 5. Итоги ---
Write-Host "`n========== СЕРВЕР ==========" -ForegroundColor Cyan
Get-Content $srvOut -Raw -Encoding UTF8

for ($i = 1; $i -le $Players; $i++) {
  Write-Host "========== ИГРОК $i ==========" -ForegroundColor Cyan
  Get-Content (Join-Path $env:TEMP "ou-play-client$i.out") -Raw -Encoding UTF8
}

Write-Host "=========================================" -ForegroundColor Cyan
Write-Host " Готово. Чтобы посмотреть мир глазами:" -ForegroundColor Green
Write-Host "   cargo run -p ou-render --release --bin ou-netrender -- 127.0.0.1:$Port 8 4"
Write-Host "   (сохранит кадры в render-net.png)" -ForegroundColor Green
Write-Host "=========================================" -ForegroundColor Cyan
