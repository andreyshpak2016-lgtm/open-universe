# Кросс-проверка рельефа: C++-формула клиента против эталона Rust-сервера.
#
# Запуск: powershell -ExecutionPolicy Bypass -File tools/test-terrain.ps1
#
# Что делает:
# 1. Rust-сервер печатает эталон высот (ou-terrain-ref).
# 2. Компилирует C++-стенд (client/testdata/test_terrain.cpp) обычным MSVC.
# 3. Стенд считает те же направления общей формулой (OUTerrain.h) и сверяет.

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$server = Join-Path $root 'server'
$testdata = Join-Path $root 'client\testdata'
$ref = Join-Path $testdata 'terrain_ref.txt'
$exe = Join-Path $testdata 'test_terrain.exe'

Write-Host "== Кросс-проверка рельефа (клиент против сервера) ==" -ForegroundColor Cyan

# 1. Эталон от сервера.
Push-Location $server
try
{
  Write-Host "--> ou-terrain-ref: печатаю эталон высот" -ForegroundColor Yellow
  & cargo run -q -p ou-sim --bin ou-terrain-ref -- $ref
  if ($LASTEXITCODE -ne 0) { throw "ou-terrain-ref завершился с ошибкой" }
} finally
{
  Pop-Location
}

# 2. Компиляция C++-стенда.
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw "Не найден Visual Studio с C++-инструментами" }
$vcvars = Join-Path $vs 'VC\Auxiliary\Build\vcvars64.bat'
if (-not (Test-Path $vcvars)) { throw "Не найден vcvars64.bat: $vcvars" }

Write-Host "--> компиляция test_terrain.cpp (MSVC)" -ForegroundColor Yellow
$bat = Join-Path $env:TEMP 'ou_build_test_terrain.bat'
$includeDir = Join-Path $root 'client\Source\OpenUniverse'
@"
@echo off
call "$vcvars" >nul 2>&1
cl /nologo /std:c++20 /W3 /EHsc /I"$includeDir" /Fe:"$exe" "$(Join-Path $testdata 'test_terrain.cpp')" /Fo:"$testdata\\"
"@ | Set-Content -Path $bat -Encoding ASCII
cmd /c $bat
if ($LASTEXITCODE -ne 0) { throw "Компиляция не удалась" }

# 3. Прогон.
Write-Host "--> запуск проверки" -ForegroundColor Yellow
& $exe $ref
$code = $LASTEXITCODE
Write-Host ""
if ($code -eq 0)
{
  Write-Host "Рельеф клиента подтверждён на эталоне сервера." -ForegroundColor Green
} else
{
  Write-Host "Проверка провалена." -ForegroundColor Red
}
exit $code
