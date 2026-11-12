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

# 2a. Лаунчер (C#) — вход, новости, автообновление.
try {
  & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'build-launcher.ps1')
} catch {
  Write-Host "Лаунчер не собран: $_" -ForegroundColor DarkYellow
}

# 3. Упаковка
Write-Host "--> упаковка артефакта" -ForegroundColor Yellow
# VERSION пишем ДО сборки установщиков: установщик кладёт этот файл в игру,
# и лаунчер по нему определяет установленную версию.
Set-Content -Path (Join-Path $root 'VERSION') -Value $Version -NoNewline -Encoding UTF8
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
  (Join-Path $server 'target\release\ou-netrender.exe'),
  (Join-Path $server 'target\release\ou-landing-demo.exe'),
  (Join-Path $server 'target\release\ou-script-demo.exe'),
  (Join-Path $server 'target\release\ou-market-sim.exe'),
  (Join-Path $server 'target\release\ou-build-demo.exe'),
  (Join-Path $server 'target\release\ou-dump-packet.exe'),
  (Join-Path $root 'tools\play.ps1'),
  (Join-Path $root 'tools\server.ps1'),
  (Join-Path $root 'tools\game.ps1'),
  (Join-Path $root 'tools\ou-common.ps1'),
  (Join-Path $root 'launcher\OpenUniverseLauncher.exe'),
  (Join-Path $root 'play.bat'),
  (Join-Path $root 'server.bat'),
  (Join-Path $root 'game.bat'),
  (Join-Path $root 'README.md')
)
Compress-Archive -Path $files -DestinationPath $zip -Force
Write-Host ("Артефакт: {0} ({1:N0} байт)" -f $zip, (Get-Item $zip).Length) -ForegroundColor Green

# 3a. Установщик (Inno Setup): один .exe, который всё ставит и делает ярлыки.
$setup = Join-Path $dist "OpenUniverse-Setup-$Version.exe"
$gameSetup = Join-Path $dist "OpenUniverse-Game-Setup-$Version.exe"
$iscc = @(
  (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'),
  'C:\Program Files (x86)\Inno Setup 6\ISCC.exe',
  'C:\Program Files\Inno Setup 6\ISCC.exe'
) | Where-Object { Test-Path $_ } | Select-Object -First 1

if ($iscc) {
  Write-Host "--> установщик (Inno Setup)" -ForegroundColor Yellow
  & $iscc "/DMyVersion=$Version" "/DSourceRoot=$root" (Join-Path $root 'tools\installer.iss') | Out-Null
  if (Test-Path $setup) {
    Write-Host ("Установщик: {0} ({1:N1} МБ)" -f $setup, ((Get-Item $setup).Length / 1MB)) -ForegroundColor Green
  } else {
    Write-Host "Установщик не собран (проверьте ISCC)" -ForegroundColor DarkYellow
  }

  # 3b. Установщик ГРАФИЧЕСКОЙ игры — если готова сборка клиента UE5.
  $gamePkg = Join-Path $root 'client\Packaged\Windows\OpenUniverse'
  if (Test-Path (Join-Path $gamePkg '..\OpenUniverse.exe')) {
    Write-Host "--> установщик графической игры (Inno Setup)" -ForegroundColor Yellow
    & $iscc "/DMyVersion=$Version" "/DSourceRoot=$root" (Join-Path $root 'tools\installer-game.iss') | Out-Null
    if (Test-Path $gameSetup) {
      Write-Host ("Игра-установщик: {0} ({1:N1} МБ)" -f $gameSetup, ((Get-Item $gameSetup).Length / 1MB)) -ForegroundColor Green
    }
  } else {
    Write-Host "Сборки графической игры нет — пропускаю (tools/package-game.ps1)" -ForegroundColor DarkYellow
  }
} else {
  Write-Host "Inno Setup не найден — установщик пропущен (winget install JRSoftware.InnoSetup)" -ForegroundColor DarkYellow
}

# Отдельные .exe: чтобы на сайте каждая версия давала прямую ссылку на клиент,
# а не только на архив. Только реальные исполняемые файлы.
$exes = $files | Where-Object { $_ -match '\.exe$' -and (Test-Path $_) }

# 4. Обновить releases.json (список версий для сайта — без GitHub API).
try {
  & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'releases-json.ps1')
} catch {
  Write-Host "releases.json не обновлён: $_" -ForegroundColor DarkYellow
}

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
  # Прикладываем установщик игры, установщик ядра, лаунчер, exe и архив.
  # Лаунчер может попасть и в $exes (он есть в $files) — убираем дубликаты по имени.
  $upload = @($exes)
  $launcherExe = Join-Path $root 'launcher\OpenUniverseLauncher.exe'
  if (Test-Path $launcherExe) { $upload = @($launcherExe) + $upload }
  if (Test-Path $setup) { $upload = @($setup) + $upload }
  if (Test-Path $gameSetup) { $upload = @($gameSetup) + $upload }
  $upload = $upload + @($zip)
  $seen = @{}
  $upload = $upload | Where-Object {
    $name = (Split-Path -Leaf $_).ToLower()
    if ($seen.ContainsKey($name)) { $false } else { $seen[$name] = $true; $true }
  }

  # Создаём релиз без ассетов, затем грузим файлы по одному с повтором.
  # Так сбой на одном большом файле (например, игровом установщике) не теряет
  # остальные и легко повторяется.
  & $gh release create $tag `
    --repo andreyshpak2016-lgtm/open-universe `
    --title "Open Universe $Version" `
    --notes-file $notesFile
  if ($LASTEXITCODE -ne 0) {
    # Релиз уже мог существовать — продолжаем, ассеты загрузим ниже.
    Write-Host "Релиз уже существует или создан ранее — грузим ассеты." -ForegroundColor DarkYellow
  }

  foreach ($asset in $upload) {
    $name = Split-Path -Leaf $asset
    $ok = $false
    for ($try = 1; $try -le 3 -and -not $ok; $try++) {
      Write-Host ("--> загрузка {0} (попытка {1})" -f $name, $try) -ForegroundColor Yellow
      & $gh release upload $tag $asset --repo andreyshpak2016-lgtm/open-universe --clobber
      if ($LASTEXITCODE -eq 0) { $ok = $true } else { Start-Sleep -Seconds 5 }
    }
    if (-not $ok) { Write-Host ("Не удалось загрузить: {0}" -f $name) -ForegroundColor Red }
  }
  Write-Host "Готово: релиз $tag опубликован" -ForegroundColor Green
} else {
  Write-Host "Сборка готова. Для публикации запустите с -Publish." -ForegroundColor Cyan
}
