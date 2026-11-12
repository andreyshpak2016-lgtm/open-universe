# Формирует манифест игровых файлов и патч (только изменениями).
#
# Зачем: лаунчер не должен качать весь клиент заново. Манифест описывает
# каждый файл (путь, размер, SHA-256). Патч — zip только с теми файлами,
# что изменились относительно предыдущего манифеста.
#
# Использование:
#   powershell -ExecutionPolicy Bypass -File tools/make-manifest.ps1 -Version 0.34.0
#   powershell -ExecutionPolicy Bypass -File tools/make-manifest.ps1 -Version 0.35.0 `
#       -PrevManifestsDir manifests -MakePatch
#
# Результат:
#   manifests/<version>.json        — манифест этой версии (для сайта/лаунчера)
#   manifests/latest.json           — копия последнего манифеста
#   dist/patch-<version>.zip        — только изменённые файлы (если -MakePatch)

param(
  [Parameter(Mandatory = $true)][string]$Version,
  [string]$GameDir = "",
  [string]$OutDir = "",
  [switch]$MakePatch
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
if (-not $GameDir) { $GameDir = Join-Path $repo 'client\Packaged\Windows' }
if (-not $OutDir) { $OutDir = Join-Path $repo 'manifests' }
$dist = Join-Path $repo 'dist'

if (-not (Test-Path $GameDir)) { throw "нет каталога игры: $GameDir" }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
New-Item -ItemType Directory -Force -Path $dist | Out-Null

Write-Host "== Манифест Open Universe $Version ==" -ForegroundColor Cyan
Write-Host "  корень: $GameDir"

# Собираем записи о файлах (относительный путь в прямых слэшах, размер, хеш).
$rootFull = (Resolve-Path $GameDir).Path
$files = Get-ChildItem -Path $GameDir -Recurse -File | Sort-Object FullName
$entries = New-Object System.Collections.Generic.List[object]
foreach ($f in $files) {
  $rel = $f.FullName.Substring($rootFull.Length).TrimStart('\', '/') -replace '\\', '/'
  $hash = (Get-FileHash -Path $f.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
  $entries.Add([PSCustomObject]@{ p = $rel; s = [int64]$f.Length; h = $hash })
}

$manifest = [PSCustomObject]@{
  version = $Version
  files   = $entries
}
$json = $manifest | ConvertTo-Json -Depth 4 -Compress

# Предыдущий манифест читаем ДО перезаписи latest.json (нужен для патча).
$latestFile = Join-Path $OutDir 'latest.json'
$prev = $null
if (Test-Path $latestFile) {
  try {
    $prev = Get-Content $latestFile -Raw -Encoding UTF8 | ConvertFrom-Json
  } catch { $prev = $null }
}

$outFile = Join-Path $OutDir "$Version.json"
[System.IO.File]::WriteAllText($outFile, $json, (New-Object System.Text.UTF8Encoding($false)))
[System.IO.File]::WriteAllText($latestFile, $json, (New-Object System.Text.UTF8Encoding($false)))
Write-Host ("  файлов: {0}, манифест: {1} ({2:N0} байт)" -f $entries.Count, $outFile, (Get-Item $outFile).Length) -ForegroundColor Green

# --- Патч: сравнить с предыдущим манифестом и положить изменения в zip ---
if ($MakePatch) {
  $prevMap = @{}
  if ($prev -and $prev.version -ne $Version) {
    Write-Host "  сравнение с предыдущим манифестом: $($prev.version)"
    foreach ($e in $prev.files) { $prevMap[$e.p] = $e.h }
  } else {
    Write-Host "  предыдущего манифеста нет — патч будет полным" -ForegroundColor DarkYellow
  }

  $changed = New-Object System.Collections.Generic.List[object]
  foreach ($e in $entries) {
    $ph = $null
    if ($prevMap.ContainsKey($e.p)) { $ph = $prevMap[$e.p] }
    if ($ph -ne $e.h) { $changed.Add($e) }
  }

  $patch = Join-Path $dist "patch-$Version.zip"
  if (Test-Path $patch) { Remove-Item $patch -Force }

  if ($changed.Count -eq 0) {
    Write-Host "  изменений нет — патч не нужен" -ForegroundColor DarkYellow
  } else {
    $stage = Join-Path $env:TEMP ("ou-patch-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Force -Path $stage | Out-Null
    foreach ($e in $changed) {
      $src = Join-Path $GameDir ($e.p -replace '/', '\')
      $dst = Join-Path $stage ($e.p -replace '/', '\')
      $parent = Split-Path $dst -Parent
      if ($parent) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
      Copy-Item -Path $src -Destination $dst -Force
    }
    Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $patch -Force
    Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host ("  патч: {0} файлов, {1} ({2:N0} КБ)" -f $changed.Count, $patch, ((Get-Item $patch).Length / 1KB)) -ForegroundColor Green
  }
}
