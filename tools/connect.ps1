# Подключение UE5-клиента к Rust-серверу одной командой.
#
# Запускает сервер ou-netd и открывает редактор Unreal. В редакторе нажмите
# Play (Alt+P) — корабль подключится к серверу и начнёт слать ввод.
#
# Использование:
#   powershell -ExecutionPolicy Bypass -File tools/connect.ps1
#   powershell -File tools/connect.ps1 -Seconds 600 -Port 7777
#   powershell -File tools/connect.ps1 -EngineRoot "D:\UE_5.5"

param(
  [string]$EngineRoot = "",
  [int]$Seconds = 600,
  [int]$Port = 7777
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$server = Join-Path $root 'server'
$uproject = Join-Path $root 'client\OpenUniverse.uproject'

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

Write-Host "== Open Universe: сервер + UE5-клиент ==" -ForegroundColor Cyan

# 1. Собрать сервер.
Write-Host "--> сборка ou-netd" -ForegroundColor Yellow
Push-Location $server
try { & cargo build -q -p ou-server --bin ou-netd } finally { Pop-Location }

# 2. Запустить сервер в отдельном окне.
$netd = Join-Path $server 'target\debug\ou-netd.exe'
Write-Host "--> сервер: $netd 127.0.0.1:$Port $Seconds" -ForegroundColor Yellow
Start-Process -FilePath $netd -ArgumentList @("127.0.0.1:$Port", "$Seconds") -WorkingDirectory $server

# 3. Найти движок и открыть редактор.
if ($EngineRoot -and (Test-Path $EngineRoot)) { $engine = $EngineRoot } else { $engine = Find-Engine }
if (-not $engine) { throw "Unreal Engine не найден. Укажите -EngineRoot." }
$editor = Join-Path $engine 'Engine\Binaries\Win64\UnrealEditor.exe'
if (-not (Test-Path $editor)) { throw "Не найден редактор: $editor" }

Write-Host "--> редактор: $editor" -ForegroundColor Yellow
Start-Process -FilePath $editor -ArgumentList "`"$uproject`"" -WorkingDirectory (Join-Path $root 'client')

Write-Host ""
Write-Host "Готово. В редакторе нажмите Play (Alt+P)." -ForegroundColor Green
Write-Host "Проверить связь: в логе клиента появится строка" -ForegroundColor Green
Write-Host "  LogOUClient: Сервер принял: моя сущность = N" -ForegroundColor Green
