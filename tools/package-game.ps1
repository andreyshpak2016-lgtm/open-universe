# Сборка standalone-игры Open Universe (Windows x64): cook + package.
#
# Результат: client\Packaged\Windows\OpenUniverse\OpenUniverse.exe —
# запускается БЕЗ установки Unreal Engine.
#
# Использование:
#   powershell -ExecutionPolicy Bypass -File tools/package-game.ps1
#   powershell -File tools/package-game.ps1 -EngineRoot "D:\UE_5.5"

param(
  [string]$EngineRoot = ""
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$client = Join-Path $root 'client'
$uproject = Join-Path $client 'OpenUniverse.uproject'
$outDir = Join-Path $client 'Packaged'

function Find-Engine {
  $candidates = @()
  foreach ($base in @('C:\Program Files\Epic Games', 'D:\Epic Games', 'E:\Epic Games', 'D:\', 'E:\', 'C:\')) {
    if (Test-Path $base) {
      Get-ChildItem $base -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '^UE_5\.' } | ForEach-Object { $candidates += $_.FullName }
    }
  }
  $valid = @()
  foreach ($c in $candidates) {
    if (Test-Path (Join-Path $c 'Engine\Build\BatchFiles\RunUAT.bat')) { $valid += $c }
  }
  if ($valid.Count -eq 0) { return $null }
  $assoc = $null
  if (Test-Path $uproject) {
    $m = Select-String -Path $uproject -Pattern '"EngineAssociation"\s*:\s*"([^"]+)"'
    if ($m) { $assoc = $m.Matches[0].Groups[1].Value }
  }
  if ($assoc) {
    $match = @($valid | Where-Object { (Split-Path $_ -Leaf) -eq ("UE_" + $assoc) })
    if ($match.Count -gt 0) { return $match[0] }
  }
  return ($valid | Sort-Object { [version]($_ -replace '.*UE_','') } -Descending)[0]
}

if ($EngineRoot -and (Test-Path $EngineRoot)) { $engine = $EngineRoot } else { $engine = Find-Engine }
if (-not $engine) { throw "Unreal Engine не найден. Укажите -EngineRoot." }

$uat = Join-Path $engine 'Engine\Build\BatchFiles\RunUAT.bat'
# .NET из движка — для UBT.
$bundled = Get-ChildItem (Join-Path $engine 'Engine\Binaries\ThirdParty\DotNet') -Directory -ErrorAction SilentlyContinue |
  Sort-Object Name -Descending | Select-Object -First 1
if ($bundled) {
  $dn = Join-Path $bundled.FullName 'win-x64'
  if (Test-Path (Join-Path $dn 'dotnet.exe')) { $env:DOTNET_ROOT = $dn }
}

Write-Host "== Сборка игры Open Universe (Windows x64) ==" -ForegroundColor Cyan
Write-Host "Движок: $engine"
Write-Host "Проект: $uproject"
Write-Host ""

# Cook + package (standalone). Долго: компилирует и готовит ассеты.
& $uat BuildCookRun `
  -project="$uproject" `
  -noP4 -platform=Win64 -clientconfig=Shipping `
  -cook -build -stage -pak -archive `
  -archivedirectory="$outDir" `
  -utf8output

if ($LASTEXITCODE -ne 0) { throw "Сборка игры не удалась (код $LASTEXITCODE)" }

$exe = Get-ChildItem $outDir -Recurse -Filter "OpenUniverse.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
if ($exe) {
  Write-Host ""
  Write-Host "Готово: $($exe.FullName)" -ForegroundColor Green
} else {
  Write-Host "Игра собрана, но OpenUniverse.exe не найден в $outDir" -ForegroundColor Yellow
}
