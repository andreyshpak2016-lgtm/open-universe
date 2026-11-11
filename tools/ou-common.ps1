# Общие функции скриптов Open Universe.
#
# Определяют раскладку: репозиторий с исходниками (сборка через cargo)
# или установленная версия (только готовые .exe). Одни и те же скрипты
# работают в обоих случаях — поэтому после установки ничего не падает.

function Resolve-OU {
  param([string]$StartDir)

  $dirs = @()
  if ($StartDir) { $dirs += $StartDir }
  # tools/ -> корень; плюс сам каталог скрипта и рабочий каталог.
  $dirs += (Split-Path -Parent $StartDir)
  $dirs += (Get-Location).Path
  $dirs = $dirs | Where-Object { $_ } | Select-Object -Unique

  foreach ($d in $dirs) {
    # Установленная версия: exe лежат прямо рядом со скриптами/корнем.
    if (Test-Path (Join-Path $d 'ou-netd.exe')) {
      return [PSCustomObject]@{
        Root       = $d
        ExeDir     = $d
        HasSources = (Test-Path (Join-Path $d 'server\Cargo.toml'))
        World      = (Join-Path $d 'world')
      }
    }
    # Репозиторий: exe в server\target\release.
    $rel = Join-Path $d 'server\target\release'
    if (Test-Path (Join-Path $rel 'ou-netd.exe')) {
      return [PSCustomObject]@{
        Root       = $d
        ExeDir     = $rel
        HasSources = (Test-Path (Join-Path $d 'server\Cargo.toml'))
        World      = (Join-Path $d 'world')
      }
    }
    # Исходники есть, но ещё не собраны — соберём.
    if (Test-Path (Join-Path $d 'server\Cargo.toml')) {
      return [PSCustomObject]@{
        Root       = $d
        ExeDir     = $rel
        HasSources = $true
        World      = (Join-Path $d 'world')
      }
    }
  }
  return $null
}

# Каталог мира по умолчанию: рядом с exe, чтобы установка была самодостаточной.
function Resolve-OUWorld {
  param($Layout)
  if ($Layout -and $Layout.World) { return $Layout.World }
  return (Join-Path (Get-Location).Path 'world')
}
