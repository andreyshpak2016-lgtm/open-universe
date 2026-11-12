# Формирует releases.json — статический список версий игры для сайта.
#
# Зачем: сайт не должен зависеть от GitHub API (лимит 60 запросов/час на IP).
# Скрипт раз в релиз скатывает список версий в один файл рядом с сайтом.
#
# Использование:
#   powershell -ExecutionPolicy Bypass -File tools/releases-json.ps1

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$gh = "C:\Program Files\GitHub CLI\gh.exe"
$repo = "andreyshpak2016-lgtm/open-universe"
$out = Join-Path $root 'releases.json'

if (-not (Test-Path $gh)) { throw "GitHub CLI ne najden: $gh" }

Write-Host "== Spisok versij -> releases.json ==" -ForegroundColor Cyan

# ВАЖНО про кодировку: `gh` печатает UTF-8, но PowerShell 5.1 декодирует вывод
# внешних программ через консольную кодовую страницу (OEM, напр. CP866) — из-за
# этого кириллица в notes превращалась в «кракозябры». Поэтому просим gh писать
# во временный файл через cmd (байты как есть), а читаем файл строго как UTF-8.
$tmp = Join-Path $env:TEMP ("ou-releases-" + [guid]::NewGuid().ToString("N") + ".json")
try {
  $cmd = '"{0}" api "repos/{1}/releases?per_page=100" > "{2}" 2>nul' -f $gh, $repo, $tmp
  & $env:ComSpec /c $cmd
  if (-not (Test-Path $tmp)) { throw "gh api releases: пустой ответ" }
  $json = [System.IO.File]::ReadAllText($tmp, [System.Text.Encoding]::UTF8)
} finally {
  if (Test-Path $tmp) { Remove-Item $tmp -Force -ErrorAction SilentlyContinue }
}
if (-not $json) { throw "gh api releases: пустой ответ" }
# Вытаскиваем код возврата gh нельзя (вывод ушёл в файл) — проверяем, что JSON
# разбирается и это массив релизов.
try {
  $rels = $json | ConvertFrom-Json
} catch {
  throw "gh api releases: некорректный JSON: $_"
}
if (-not $rels) { throw "gh api releases: список пуст" }
$latestTag = ""
if ($rels.Count -gt 0) { $latestTag = $rels[0].tag_name }

# Сначала проходим от старых к новым и запоминаем последний встреченный
# установщик игры: свежие релизы часто содержат только лаунчер, и лаунчеру
# нужен установщик из ближайшего более СТАРОГО релиза.
$ordered = @($rels)              # API отдаёт новейшие первыми
[array]::Reverse($ordered)       # теперь от старых к новым
$carried = @{}                   # tag -> @{ url; size; tag }
$lastUrl = $null; $lastSize = 0; $lastTag = $null
foreach ($rel in $ordered) {
  $assets = @($rel.assets)
  $gs = ($assets | Where-Object { $_.name -match '^OpenUniverse-Game-Setup.*\.exe$' } | Select-Object -First 1)
  if ($gs) { $lastUrl = $gs.browser_download_url; $lastSize = [int64]$gs.size; $lastTag = $rel.tag_name }
  $carried[$rel.tag_name] = [PSCustomObject]@{ Url = $lastUrl; Size = $lastSize; Tag = $lastTag }
}

$list = @()
$relIndex = 0
foreach ($rel in $rels) {
  $assets = @($rel.assets)
  $setup     = ($assets | Where-Object { $_.name -match '^OpenUniverse-Setup.*\.exe$' } | Select-Object -First 1)
  $client    = ($assets | Where-Object { $_.name -match '^ou-netclient.*\.exe$' } | Select-Object -First 1)
  $launcher  = ($assets | Where-Object { $_.name -eq 'OpenUniverseLauncher.exe' } | Select-Object -First 1)
  $zip       = ($assets | Where-Object { $_.name -match '\.zip$' } | Select-Object -First 1)
  $carry     = $carried[$rel.tag_name]

  $primary = if ($launcher) { $launcher } elseif ($setup) { $setup } elseif ($zip) { $zip }

  # Текст новостей: тело релиза (обычно CHANGELOG). Держим только у свежих
  # релизов и обрезаем — чтобы releases.json не разрастался.
  $notes = ""
  if ($relIndex -lt 8 -and $rel.body) {
    $notes = [string]$rel.body
    if ($notes.Length -gt 1500) { $notes = $notes.Substring(0, 1500) }
  }
  $relIndex++

  $list += [PSCustomObject]@{
    tag          = $rel.tag_name
    name         = $rel.name
    notes        = $notes
    date         = $rel.published_at
    latest       = ($rel.tag_name -eq $latestTag)
    prerelease   = [bool]$rel.prerelease
    size         = if ($carry -and $carry.Size) { [int64]$carry.Size } elseif ($primary) { [int64]$primary.size } else { 0 }
    gameSetup    = if ($carry) { $carry.Url } else { $null }
    gameSetupTag = if ($carry) { $carry.Tag } else { $null }
    setup        = if ($setup) { $setup.browser_download_url } else { $null }
    setupSize    = if ($setup) { [int64]$setup.size } else { 0 }
    launcher     = if ($launcher) { $launcher.browser_download_url } else { $null }
    launcherSize = if ($launcher) { [int64]$launcher.size } else { 0 }
    client       = if ($client) { $client.browser_download_url } else { $null }
    zip          = if ($zip) { $zip.browser_download_url } else { $null }
    page         = "https://github.com/$repo/releases/tag/$($rel.tag_name)"
  }
}

$doc = [PSCustomObject]@{
  generated = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
  repo      = $repo
  releases  = $list
}

$jsonText = $doc | ConvertTo-Json -Depth 6
[System.IO.File]::WriteAllText($out, $jsonText, (New-Object System.Text.UTF8Encoding($false)))
Write-Host ("Gotovo: {0} ({1} versij, {2:N0} bajt)" -f $out, $list.Count, (Get-Item $out).Length) -ForegroundColor Green
