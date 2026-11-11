# Публикация релиза одной командой.
#
# Использование (после того как репозиторий создан на GitHub):
#   powershell -ExecutionPolicy Bypass -File tools/publish.ps1 -Version 0.1.0
#
# Скрипт: запускает тесты, собирает релиз, делает zip, коммитит, ставит тег,
# пушит и создаёт GitHub Release с артефактом.

param([string]$Version = (Get-Content (Join-Path (Split-Path -Parent $PSScriptRoot) 'VERSION') -Raw).Trim())

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$gh = "C:\Program Files\GitHub CLI\gh.exe"
$repo = "andreyshpak2016-lgtm/open-universe"
$tag = "v$Version"

Write-Host "== Публикация Open Universe $tag ==" -ForegroundColor Cyan

# 1. Репозиторий должен существовать
& $gh repo view $repo 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) {
  Write-Host "Репозиторий $repo не найден. Создайте его на github.com/new (публичный, без README/.gitignore)." -ForegroundColor Red
  exit 1
}

# 2. Сборка и упаковка
& powershell -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'release.ps1') -Version $Version
$zip = Join-Path $root "dist\open-universe-$Version-windows-x64.zip"

# 3. Пуш
Write-Host "--> git push" -ForegroundColor Yellow
& git -C $root add -A
& git -C $root commit -m "release: $tag" 2>$null
& git -C $root push -u origin main
& git -C $root tag -a $tag -m "Open Universe $Version" 2>$null
& git -C $root push origin $tag

# 4. Релиз
Write-Host "--> gh release create $tag" -ForegroundColor Yellow
& $gh release create $tag $zip --repo $repo --title "Open Universe $Version" --notes-file (Join-Path $root 'CHANGELOG.md') 2>&1
if ($LASTEXITCODE -ne 0) {
  # если релиз уже есть — просто догрузим артефакт
  & $gh release upload $tag $zip --repo $repo --clobber
}
Write-Host "Готово: https://github.com/$repo/releases/tag/$tag" -ForegroundColor Green
