# Тесты ядра Open Universe (Rust).
#
# Запускает весь workspace-набор тестов: симуляция, рельеф, бой, экономика,
# протокол, сервер, шардирование. Не требует Unreal Engine.
#
# Использование:
#   powershell -ExecutionPolicy Bypass -File tools/test-rust.ps1
#
# Результат: сводка по числу тестов и код возврата 0/1.

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
$server = Join-Path $root 'server'

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
  Write-Host "cargo не найден. Установите Rust: https://rustup.rs" -ForegroundColor Red
  exit 1
}

Write-Host "== Тесты ядра (Rust) ==" -ForegroundColor Cyan
Push-Location $server
try {
  $out = cargo test --workspace 2>&1
  $out | Out-Host
} finally {
  Pop-Location
}

$failed = ($out | Select-String 'test result: FAILED').Count
$passed = 0
foreach ($l in ($out | Select-String 'test result: ok\.')) {
  if ($l.Line -match 'ok\.\s+(\d+) passed') { $passed += [int]$Matches[1] }
}
Write-Host ("тестов пройдено: {0}, провалов блоков: {1}" -f $passed, $failed) -ForegroundColor Yellow

if ($failed -gt 0) { Write-Host "Тесты ядра: FAILED" -ForegroundColor Red; exit 1 }
Write-Host "Тесты ядра: OK" -ForegroundColor Green
exit 0
