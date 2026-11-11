# Проверка клиентского протокола на реальном пакете сервера.
#
# Запуск: powershell -ExecutionPolicy Bypass -File tools/test-client.ps1
#
# Что делает:
# 1. Просит Rust-сервер сохранить реальный сетевой пакет (ou-dump-packet).
# 2. Компилирует C++-стенд (client/testdata/test_wire.cpp) обычным MSVC.
# 3. Прогоняет стенд: он декодирует пакет и сверяет значения с эталоном.

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$server = Join-Path $root 'server'
$testdata = Join-Path $root 'client\testdata'
$packet = Join-Path $testdata 'packet.dat'
$exe = Join-Path $testdata 'test_wire.exe'

Write-Host "== Проверка клиентского протокола ==" -ForegroundColor Cyan

# 1. Пакет от сервера.
Push-Location $server
try {
  Write-Host "--> ou-dump-packet: сохраняю реальный пакет" -ForegroundColor Yellow
  & cargo run -q --release -p ou-server --bin ou-dump-packet -- $packet 60
  if ($LASTEXITCODE -ne 0) { throw "ou-dump-packet завершился с ошибкой" }
} finally {
  Pop-Location
}

# 2. Компиляция C++-стенда.
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw "Не найден Visual Studio с C++-инструментами" }
$vcvars = Join-Path $vs 'VC\Auxiliary\Build\vcvars64.bat'
if (-not (Test-Path $vcvars)) { throw "Не найден vcvars64.bat: $vcvars" }

Write-Host "--> компиляция test_wire.cpp (MSVC)" -ForegroundColor Yellow
$bat = Join-Path $env:TEMP 'ou_build_test_wire.bat'
$includeDir = Join-Path $root 'client\Source\OpenUniverse'
@"
@echo off
call "$vcvars" >nul 2>&1
cl /nologo /std:c++20 /W3 /EHsc /I"$includeDir" /Fe:"$exe" "$(Join-Path $testdata 'test_wire.cpp')" /Fo:"$testdata\\"
"@ | Set-Content -Path $bat -Encoding ASCII
cmd /c $bat
if ($LASTEXITCODE -ne 0) { throw "Компиляция не удалась" }

# 3. Прогон.
Write-Host "--> запуск проверки" -ForegroundColor Yellow
& $exe $packet
$code = $LASTEXITCODE
Write-Host ""
if ($code -eq 0) {
  Write-Host "Клиентский протокол подтверждён на реальном пакете сервера." -ForegroundColor Green
} else {
  Write-Host "Проверка провалена." -ForegroundColor Red
}
exit $code
