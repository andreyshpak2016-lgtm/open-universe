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
$web = Join-Path $fw 'System.Web.Extensions.dll'
if (-not (Test-Path $sys)) { throw "Не найден $sys" }
if (-not (Test-Path $web)) { throw "Не найден $web" }

# Значок приложения (собирается tools/make-icon.ps1 при необходимости).
$ico = Join-Path $root 'assets\openuniverse.ico'

# Версия из VERSION — пишем в свойства exe (Файл → Свойства → Подробно).
$verRaw = '0.0.0'
$vfile = Join-Path $root 'VERSION'
if (Test-Path $vfile) { $verRaw = (Get-Content $vfile -Raw).Trim() }
$parts = @($verRaw -split '\.')
while ($parts.Count -lt 4) { $parts += '0' }
$ver4 = ($parts[0..3] -join '.')

$infoPath = Join-Path $root 'launcher\AssemblyVersion.cs'
$info = @"
using System.Reflection;
[assembly: AssemblyTitle("Open Universe Launcher")]
[assembly: AssemblyProduct("Open Universe")]
[assembly: AssemblyCompany("Open Universe")]
[assembly: AssemblyDescription("Open Universe launcher")]
[assembly: AssemblyCopyright("Open Universe")]
[assembly: AssemblyVersion("$ver4")]
[assembly: AssemblyFileVersion("$ver4")]
[assembly: AssemblyInformationalVersion("$verRaw")]
"@
[System.IO.File]::WriteAllText($infoPath, $info, (New-Object System.Text.UTF8Encoding($true)))

Write-Host "== Сборка лаунчера ==" -ForegroundColor Cyan
$iconArg = if (Test-Path $ico) { "/win32icon:$ico" } else { $null }
& $csc /nologo /target:winexe /optimize+ "/out:$out" $iconArg `
  "/r:$sys" "/r:$drw" "/r:$web" `
  /r:System.dll /r:System.Core.dll /r:System.Net.Http.dll `
  "$src" "$infoPath"
if ($LASTEXITCODE -ne 0) { throw "Компиляция не удалась" }

if (Test-Path $out) {
  Write-Host ("Готово: {0} ({1:N0} КБ)" -f $out, ((Get-Item $out).Length / 1KB)) -ForegroundColor Green
} else {
  throw "Лаунчер не собран"
}
