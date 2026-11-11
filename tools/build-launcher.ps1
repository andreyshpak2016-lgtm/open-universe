# Сборка лаунчера Open Universe (C#, .NET Framework, без зависимостей).
#
# Результат: launcher\OpenUniverseLauncher.exe
#
# Использование:
#   powershell -ExecutionPolicy Bypass -File tools/build-launcher.ps1

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$src = Join-Path $root 'launcher\Launcher.cs'
$out = Join-Path $root 'launcher\OpenUniverseLauncher.exe'

# Компилятор C# из .NET Framework (есть в Windows).
$csc = @(
  (Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'),
  (Join-Path $env:WINDIR 'Microsoft.NET\Framework\v4.0.30319\csc.exe')
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $csc) { throw "csc.exe не найден (.NET Framework)" }

$fw = Split-Path $csc   # ...\v4.0.30319 (csc лежит прямо в нём)
$sys = Join-Path $fw 'System.Windows.Forms.dll'
$drw = Join-Path $fw 'System.Drawing.dll'
if (-not (Test-Path $sys)) { throw "Не найден $sys" }

Write-Host "== Сборка лаунчера ==" -ForegroundColor Cyan
& $csc /nologo /target:winexe /optimize+ "/out:$out" `
  "/r:$sys" "/r:$drw" `
  /r:System.dll /r:System.Core.dll /r:System.Net.Http.dll `
  "$src"
if ($LASTEXITCODE -ne 0) { throw "Компиляция не удалась" }

if (Test-Path $out) {
  Write-Host ("Готово: {0} ({1:N0} КБ)" -f $out, ((Get-Item $out).Length / 1KB)) -ForegroundColor Green
} else {
  throw "Лаунчер не собран"
}
