# Скрипт релиза Open Universe.
#
# Сборка:
#   powershell -File tools/release.ps1 -Version 0.1.0
# Публикация на GitHub (нужен установленный и авторизованный gh):
#   powershell -File tools/release.ps1 -Version 0.1.0 -Publish
#
# Что делает:
#   1. Проверяет, что тег vX.Y.Z ещё не существует.
#   2. Собирает release-бинарники (cargo build --release).
#   3. Упаковывает их в dist/open-universe-<ver>-windows-x64.zip.
#   4. Пишет версию в VERSION и добавляет запись в CHANGELOG (если -Bump).
#   5. При -Publish: коммитит, ставит тег, пушит и создаёт релиз с артефактом.

param(
  [Parameter(Mandatory = $true)][string]$Version,
  [switch]$Publish,
  [switch]$Bump
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$server = Join-Path $root 'server'
$dist = Join-Path $root 'dist'
$gh = "C:\Program Files\GitHub CLI\gh.exe"

if ($Version -notmatch '^\d+\.\d+\.\d+$') {
  throw "Версия должна быть в формате X.Y.Z (например 0.1.0), получено: $Version"
}
$tag = "v$Version"

Write-Host "== Open Universe $tag ==" -ForegroundColor Cyan

# 1. Тег не должен существовать
if ($Publish) {
  $existing = & git -C $root tag --list $tag
  if ($existing) { throw "Тег $tag уже существует" }
}

# 2. Сборка
Write-Host "--> cargo build --release" -ForegroundColor Yellow
Push-Location $server
try {
  & cargo test --workspace
  if ($LASTEXITCODE -ne 0) { throw "Тесты не прошли" }
  & cargo build --release --workspace
  if ($LASTEXITCODE -ne 0) { throw "Сборка не удалась" }
} finally {
  Pop-Location
}

# 3. Упаковка
Write-Host "--> упаковка артефакта" -ForegroundColor Yellow
New-Item -ItemType Directory -Force -Path $dist | Out-Null
$zip = Join-Path $dist "open-universe-$Version-windows-x64.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
$files = @(
  (Join-Path $server 'target\release\ou-server.exe'),
  (Join-Path $server 'target\release\ou-client.exe'),
  (Join-Path $server 'target\release\ou-netd.exe'),
  (Join-Path $server 'target\release\ou-netclient.exe'),
  (Join-Path $server 'target\release\ou-worldgen.exe'),
  (Join-Path $server 'target\release\ou-shard-demo.exe'),
  (Join-Path $server 'target\release\ou-persist-demo.exe'),
  (Join-Path $server 'target\release\ou-market-demo.exe'),
  (Join-Path $server 'target\release\ou-combat-demo.exe'),
  (Join-Path $server 'target\release\ou-render.exe'),
  (Join-Path $root 'README.md')
)
Compress-Archive -Path $files -DestinationPath $zip -Force
Write-Host ("Артефакт: {0} ({1:N0} байт)" -f $zip, (Get-Item $zip).Length) -ForegroundColor Green

# 4. Обновить VERSION
Set-Content -Path (Join-Path $root 'VERSION') -Value $Version -NoNewline -Encoding UTF8

# 5. Публикация
if ($Publish) {
  if (-not (Test-Path $gh)) { throw "GitHub CLI не найден: $gh" }
  Write-Host "--> git commit + tag + push" -ForegroundColor Yellow
  & git -C $root add -A
  & git -C $root commit -m "release: $tag"
  & git -C $root tag -a $tag -m "Open Universe $Version"
  & git -C $root push origin HEAD
  & git -C $root push origin $tag

  Write-Host "--> gh release create $tag" -ForegroundColor Yellow
  $notesFile = Join-Path $root 'CHANGELOG.md'
  & $gh release create $tag $zip `
    --repo andreyshpak2016-lgtm/open-universe `
    --title "Open Universe $Version" `
    --notes-file $notesFile
  Write-Host "Готово: релиз $tag опубликован" -ForegroundColor Green
} else {
  Write-Host "Сборка готова. Для публикации запустите с -Publish." -ForegroundColor Cyan
}
