# Универсальный помощник проверки UE5-клиента Open Universe.
#
# Один инструмент для всего: собрать/запустить клиент, снять кадр (графика),
# проверить работоспособность по логу и ответить «что реализовано, а что нет».
#
# Режимы:
#   * headless (по умолчанию) — `-RenderOffscreen -nosound -OULateShot`,
#     кадр пишет сам движок в client\Saved\Screenshots; окно на экран НЕ выводится.
#   * `-Window` — реальное окно + кадр через GDI (как в test-shipping.ps1).
#   * `-Shipping` — проверяем упакованную Shipping-сборку (та, что у игрока).
#   * `-Preset <имя>` — готовый сценарий проверки одной фичи (см. -List).
#   * `-Preset all` — прогон ВСЕХ сценариев подряд: таблица «фича → статус».
#
# Примеры:
#   powershell -ExecutionPolicy Bypass -File tools/capture.ps1 -List
#   powershell -ExecutionPolicy Bypass -File tools/capture.ps1 -Preset build
#   powershell -ExecutionPolicy Bypass -File tools/capture.ps1 -Preset workshop -Window
#   powershell -ExecutionPolicy Bypass -File tools/capture.ps1 -Preset all
#   powershell -ExecutionPolicy Bypass -File tools/capture.ps1 -Flag "-OUAutoOrg" -Name org
#   powershell -ExecutionPolicy Bypass -File tools/capture.ps1 -Shipping -Window
#
# Результат: shots\capture-<имя>.png + код возврата 0/1.

param(
  [string[]]$Flag = @(),
  [string]$Name = "",
  [string]$Preset = "",
  [switch]$Window,
  [switch]$Shipping,
  [switch]$NoServer,
  [switch]$Dx11,
  [int]$WaitSec = 0,
  [string]$Pilot = "TestPilot",
  [string[]]$Expect = @(),
  [switch]$KeepOpen,
  [switch]$List
)

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
$client = Join-Path $root 'client'
$proj = Join-Path $client 'OpenUniverse.uproject'
$shots = Join-Path $root 'shots'
$log = Join-Path $client 'Saved\Logs\OpenUniverse.log'
New-Item -ItemType Directory -Force -Path $shots | Out-Null

# --- Сценарии проверки фич: флаги запуска + маркеры в логе ---
# Cats  — категории лога, которые обязаны присутствовать (значит модуль живой).
# Expect— подстроки, которые обязаны быть в логе (значит фича реально сработала).
$Presets = @(
  [pscustomobject]@{ Name='smoke';     Flags=@('-OUAutoConnect');                                     Cats=@('LogOUPlanet','LogOUBase','LogOUMenu','LogOUGameMode'); Expect=@('Сцена Open Universe','Тела системы построены'); Wait=38; Desc='Запуск, карта, планета, база, меню, привязка к серверу' }
  [pscustomobject]@{ Name='build';     Flags=@('-OUAutoConnect','-OUAutoBuild');                      Cats=@('LogOUChar'); Expect=@('Меню строительства: открыто','Строительство: поставить'); Wait=40; Desc='Строительство: меню, постановка/покраска/удаление блоков' }
  [pscustomobject]@{ Name='dig';       Flags=@('-OUAutoConnect','-OUAutoDig');                        Cats=@('LogOUPlanet'); Expect=@('Копание: слот','Добыто'); Wait=40; Desc='Копание: авторитетные воронки и добыча руды' }
  [pscustomobject]@{ Name='trade';     Flags=@('-OUAutoConnect','-OUAutoTrade');                      Cats=@('LogOUMenu'); Expect=@('Рынок: открыт','Рынок:'); Wait=40; Desc='Рынок: окно, продажа и покупка' }
  [pscustomobject]@{ Name='craft';     Flags=@('-OUAutoConnect','-OUAutoCraft');                      Cats=@('LogOUClient'); Expect=@('Каталог рецептов'); Wait=40; Desc='Производство: каталог рецептов и крафт' }
  [pscustomobject]@{ Name='fire';      Flags=@('-OUAutoConnect','-OUAutoFire');                       Cats=@('LogOUShip'); Expect=@('Бой: выстрел'); Wait=40; Desc='Бой: выстрелы корабля и попадания' }
  [pscustomobject]@{ Name='script';    Flags=@('-OUAutoConnect','-OUAutoScript');                     Cats=@('LogOUChar'); Expect=@('Скрипты: открыто','Скрипты:'); Wait=45; Desc='Скриптинг: окно, привязка шаблона, вывод log()' }
  [pscustomobject]@{ Name='blueprint'; Flags=@('-OUAutoConnect','-OUAutoBlueprint');                   Cats=@('LogOUChar'); Expect=@('Шаблон: поставлен'); Wait=45; Desc='Шаблоны построек: постановка куба' }
  [pscustomobject]@{ Name='workshop';  Flags=@('-OUAutoConnect','-OUAutoWorkshop');                    Cats=@('LogOUClient'); Expect=@('Мастерская: открыто','Мастерская: публикую'); Wait=45; Desc='Мастерская: публикация и витрина шаблонов' }
  [pscustomobject]@{ Name='org';       Flags=@('-OUAutoConnect','-OUAutoOrg');                         Cats=@('LogOUClient'); Expect=@('Организация: открыто','Организация: создаю'); Wait=40; Desc='Организации: окно и создание гильдии' }
  [pscustomobject]@{ Name='chat';      Flags=@('-OUAutoConnect','-OUAutoChat');                        Cats=@('LogOUClient'); Expect=@('Чат:'); Wait=40; Desc='Чат: отправка и приём сообщений' }
  [pscustomobject]@{ Name='talents';   Flags=@('-OUAutoConnect','-OUAutoTalents');                     Cats=@('LogOUClient'); Expect=@('Таланты: открыто','Прогресс: уровень'); Wait=45; Desc='Прогрессия: опыт, уровни, окно талантов' }
  [pscustomobject]@{ Name='walk';      Flags=@('-OUAutoConnect','-OUAutoWalk');                        Cats=@('LogOUChar','LogOUGameMode'); Expect=@(); Wait=38; Desc='Пеший персонаж: движение и анимация' }
  [pscustomobject]@{ Name='ship';      Flags=@('-OUAutoConnect','-OUStartInShip');                     Cats=@('LogOUShip'); Expect=@(); Wait=40; Desc='Корабль: старт в корабле, полёт' }
  [pscustomobject]@{ Name='system';    Flags=@('-OUAutoConnect','-OUMoonOrbit');                       Cats=@('LogOUSolar'); Expect=@('Тела системы построены'); Wait=40; Desc='Звёздная система: тела, луна, орбита' }
  [pscustomobject]@{ Name='belt';      Flags=@('-OUAutoConnect','-OUStartInShip','-OUBeltOrbit');      Cats=@('LogOUSolar'); Expect=@('Пояс астероидов: камней'); Wait=44; Desc='Пояс астероидов: кольцо камней (генерация системы)' }
  [pscustomobject]@{ Name='combat';    Flags=@('-OUAutoConnect','-OUCombatDemo');                      Cats=@('LogOUShip'); Expect=@('Бой: выстрел'); Wait=40; Desc='Бой: демо-сражение у тела' }
  [pscustomobject]@{ Name='pause';     Flags=@('-OUAutoConnect','-OUPauseMenu');                       Cats=@('LogOUMenu'); Expect=@('ESC'); Wait=38; Desc='Пауза-меню (ESC)' }
  [pscustomobject]@{ Name='base';      Flags=@('-OUAutoConnect','-OUBaseOverview');                    Cats=@('LogOUBase'); Expect=@('Комплекс построен'); Wait=38; Desc='Обзор базы: посадочное поле и строения' }
  [pscustomobject]@{ Name='systems';   Flags=@('-OUAutoConnect','-OUStartInShip','-OUAutoFire');       Cats=@('LogOUClient'); Expect=@('Системы: трюм'); Wait=42; Desc='Бортовые системы: трюм, топливо, энергия (модули)' }
  [pscustomobject]@{ Name='voxels';    Flags=@('-OUAutoConnect','-OUVoxels');                          Cats=@('LogOUChar'); Expect=@(); Wait=38; Desc='Окно вокселей (V): квадраты размера кисти' }
  [pscustomobject]@{ Name='collide';   Flags=@('-OUAutoConnect','-OUAutoBuild','-OUWalkInto','-OUGhostLog'); Cats=@('LogOUChar'); Expect=@('Столкновение'); Wait=55; Desc='Коллизия: персонаж упирается в постройку и не проходит сквозь' }
  [pscustomobject]@{ Name='menus';     Flags=@('-OUAutoConnect','-OUAutoMenus');                       Cats=@('LogOUMenu','LogOUChar'); Expect=@('Скрипты: открыто','Воксели: окно открыто'); Wait=50; Desc='Все окна по очереди: рынок, скрипты, мастерская, организация, персонаж, воксели' }
  [pscustomobject]@{ Name='cores';     Flags=@('-OUAutoConnect','-OUCore','-OUCoreKinds');             Cats=@('LogOUClient','LogOUChar'); Expect=@('16x16'); Wait=48; Desc='Ядра: несколько ядер с лимитами (2 базы / 3 корабля / 2 динамич.), активное ядро' }
)

if ($List) {
  Write-Host "Сценарии проверки (tools/capture.ps1 -Preset <имя>):" -ForegroundColor Cyan
  foreach ($p in $Presets) {
    Write-Host ("  {0,-10} {1}" -f $p.Name, $p.Desc)
  }
  Write-Host ("  {0,-10} {1}" -f 'all', 'Прогнать все сценарии и выдать таблицу (фича -> статус)')
  Write-Host ""
  Write-Host "Доп. ключи: -Window (кадр через окно), -Shipping (упакованная сборка), -Flag '<строка>' (свои флаги)." -ForegroundColor DarkGray
  exit 0
}

# --- Поиск движка и сборки ---
function Find-Engine {
  foreach ($base in @('D:\UE_5.5','C:\Program Files\Epic Games\UE_5.5','D:\Epic Games\UE_5.5')) {
    if (Test-Path (Join-Path $base 'Engine\Binaries\Win64\UnrealEditor.exe')) { return $base }
  }
  Get-ChildItem 'D:\','C:\','E:\' -Directory -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -match '^UE_5\.' } |
    Where-Object { Test-Path (Join-Path $_.FullName 'Engine\Binaries\Win64\UnrealEditor.exe') } |
    Select-Object -First 1 -ExpandProperty FullName
}

function Resolve-ShippingExe {
  $cand = @((Join-Path $env:LOCALAPPDATA 'Programs\Open Universe'), (Join-Path $root 'client\Packaged\Windows'))
  foreach ($d in $cand) {
    $ship = Join-Path $d 'OpenUniverse\Binaries\Win64\OpenUniverse-Win64-Shipping.exe'
    if (Test-Path $ship) { return $ship }
  }
  return $null
}

$serverExe = Join-Path $root 'server\target\debug\ou-netd.exe'
if (-not (Test-Path $serverExe)) { $serverExe = Join-Path $root 'server\target\release\ou-netd.exe' }

# --- GDI: кадр с реального окна ---
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class OUcap {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
Add-Type -AssemblyName System.Drawing

function Stop-OuProcs {
  for ($i = 0; $i -lt 5; $i++) {
    $procs = Get-Process UnrealEditor,UnrealEditor-Cmd,OpenUniverse,OpenUniverse-Win64-Shipping,CrashReportClient,CrashReportClientEditor,ou-netd -ErrorAction SilentlyContinue
    if (-not $procs) { break }
    $procs | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 800
  }
}

# --- Метрики кадра: графическая составляющая ---
function Measure-Frame([string]$Path) {
  $res = [pscustomobject]@{ Width=0; Height=0; Lit=0.0; Unique=0; Mean=0.0; Ok=$false }
  if (-not (Test-Path $Path)) { return $res }
  $img = [System.Drawing.Bitmap]::FromFile($Path)
  try {
    $res.Width = $img.Width; $res.Height = $img.Height
    $stepX = [Math]::Max(1, [int]($img.Width / 64)); $stepY = [Math]::Max(1, [int]($img.Height / 64))
    $total = 0; $lit = 0; $sum = 0.0
    $colors = @{}
    for ($y = 0; $y -lt $img.Height; $y += $stepY) {
      for ($x = 0; $x -lt $img.Width; $x += $stepX) {
        $c = $img.GetPixel($x, $y); $total++
        $lum = ($c.R + $c.G + $c.B) / 3.0
        $sum += $lum
        if (($c.R + $c.G + $c.B) -gt 45) { $lit++ }
        $key = ('{0}-{1}-{2}' -f ([int]($c.R/16)), ([int]($c.G/16)), ([int]($c.B/16)))
        $colors[$key] = 1
      }
    }
    if ($total -gt 0) { $res.Lit = [double]$lit / $total; $res.Mean = $sum / $total }
    $res.Unique = $colors.Count
  } finally { $img.Dispose() }
  $res.Ok = ($res.Lit -gt 0.05 -and $res.Unique -gt 6)
  return $res
}

# --- Логовые проверки: работоспособность ---
function Test-ClientLog([string]$LogPath, [string[]]$WantCats, [string[]]$WantText) {
  $r = [pscustomobject]@{ Exists=$false; MissingCats=@(); MissingText=@(); Errors=@(); Lines=0; Text='' }
  if (-not (Test-Path $LogPath)) { return $r }
  $r.Exists = $true
  $text = Get-Content $LogPath -Raw -Encoding UTF8
  $r.Text = $text
  $r.Lines = ([regex]::Matches($text, "`n")).Count + 1
  foreach ($c in $WantCats) { if ($text -notmatch [regex]::Escape($c)) { $r.MissingCats += $c } }
  foreach ($t in $WantText) { if ($text -notmatch [regex]::Escape($t)) { $r.MissingText += $t } }
  # Ошибки модулей игры: строки вида "LogOU*: Error:"
  foreach ($m in [regex]::Matches($text, '(?m)^.*Log(OU|OpenUniverse)[A-Za-z]*: Error:.*$')) { $r.Errors += $m.Value.Trim() }
  return $r
}

# --- Один прогон ---
function Invoke-Capture {
  param(
    [string]$RunName,
    [string[]]$RunFlags,
    [string[]]$WantCats,
    [string[]]$WantText,
    [int]$RunWait,
    [switch]$RunWindow,
    [switch]$RunShipping,
    [switch]$RunNoServer
  )

  if ($RunWait -le 0) { $RunWait = 40 }
  $result = [pscustomobject]@{
    Name=$RunName; Ok=$false; Shot=$null; Frame=$null; Log=$null
    Flags=($RunFlags -join ' '); Reason=''
  }

  Stop-OuProcs
  Remove-Item $log -ErrorAction SilentlyContinue
  [void][OUcap]::SetCursorPos(2, 2)

  $shotDir = Join-Path $client 'Saved\Screenshots'
  $before = @{}
  Get-ChildItem $shotDir -Recurse -Filter '*.png' -ErrorAction SilentlyContinue | ForEach-Object { $before[$_.FullName] = $true }

  # Сервер
  $srv = $null
  if (-not $RunNoServer -and (Test-Path $serverExe)) {
    $worldDir = Join-Path $env:TEMP ('ou-cap-' + $RunName)
    $srv = Start-Process -FilePath $serverExe -ArgumentList '127.0.0.1:7777','120',$worldDir -PassThru -WindowStyle Hidden
    Start-Sleep -Seconds 3
  }

  # Запуск клиента
  $proc = $null
  if ($RunShipping) {
    $exe = Resolve-ShippingExe
    if (-not $exe) { $result.Reason = 'Shipping exe не найден (сначала package-game.ps1)'; return $result }
    $cliArgs = @('-windowed','-ResX=1280','-ResY=800','-nosound') + $RunFlags + ("-OUName=" + $Pilot)
    $proc = Start-Process -FilePath $exe -ArgumentList $cliArgs -PassThru -WorkingDirectory (Split-Path $exe)
  } else {
    $engine = Find-Engine
    if (-not $engine) { $result.Reason = 'UE 5.5 не найден'; return $result }
    $editor = Join-Path $engine 'Engine\Binaries\Win64\UnrealEditor.exe'
    $base = @("`"$proj`"", '-game','-nosound', ("-OUName=" + $Pilot))
    if ($RunWindow) { $base += @('-windowed','-ResX=1280','-ResY=800') }
    else            { $base += @('-RenderOffscreen') }
    # `-Dx11`: в сессии SSH (сессия 0, без рабочего стола) D3D12 не стартует —
    # DX11 работает. Нужно для удалённой съёмки на втором ПК.
    if ($Dx11)      { $base += @('-dx11') }
    if (-not ($RunFlags -contains '-OULateShot')) { $base += '-OULateShot' }
    # Кадр снимаем ближе к концу сценария, чтобы фича успела сработать.
    $base += ('-OUShotAt=' + [Math]::Max(10, $RunWait - 8))
    $proc = Start-Process -FilePath $editor -ArgumentList ($base + $RunFlags) -PassThru
  }
  Write-Host ("  запуск {0} (pid {1}), ждём не меньше {2} с..." -f $RunName, $proc.Id, $RunWait) -ForegroundColor DarkGray
  # Ждём минимум RunWait (сценарию нужно время сработать), а для headless —
  # ещё и появления отложенного кадра (первая загрузка карты бывает долгой).
  $minDeadline = (Get-Date).AddSeconds($RunWait)
  while ((Get-Date) -lt $minDeadline) { Start-Sleep -Seconds 2 }
  if (-not ($RunWindow -or $RunShipping)) {
    $deadline = (Get-Date).AddSeconds(40)
    while ((Get-Date) -lt $deadline) {
      $late = Get-ChildItem $shotDir -Recurse -Filter 'OU_late_*.png' -ErrorAction SilentlyContinue |
        Where-Object { -not $before.ContainsKey($_.FullName) }
      if ($late) { break }
      Start-Sleep -Seconds 2
    }
  }
  [void][OUcap]::SetCursorPos(2, 2)

  # Состояние процесса фиксируем ДО любых kill-операций: в Shipping-режиме
  # «жив ли» — это и есть проверка «не упал».
  $proc.Refresh()
  $exitedEarly = $proc.HasExited
  $exitCode = if ($exitedEarly) { $proc.ExitCode } else { 0 }

  # --- Кадр ---
  $outShot = Join-Path $shots ("capture-$RunName.png")
  $captured = $false
  if ($RunWindow -or $RunShipping) {
    $proc.Refresh()
    $h = $proc.MainWindowHandle
    if ($h -ne 0) {
      [void][OUcap]::ShowWindow($h, 9)
      [void][OUcap]::SetWindowPos($h, [IntPtr](-1), 40, 40, 0, 0, 0x0001 -bor 0x0010)
      [void][OUcap]::SetForegroundWindow($h)
      Start-Sleep -Seconds 3
      $r = New-Object OUcap+RECT
      [void][OUcap]::GetWindowRect($h, [ref]$r)
      $w = $r.Right - $r.Left; $ht = $r.Bottom - $r.Top
      if ($w -gt 8 -and $ht -gt 8) {
        $bmp = New-Object System.Drawing.Bitmap($w, $ht)
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.CopyFromScreen($r.Left, $r.Top, 0, 0, (New-Object System.Drawing.Size($w, $ht)))
        $bmp.Save($outShot, [System.Drawing.Imaging.ImageFormat]::Png)
        $g.Dispose(); $bmp.Dispose()
        $captured = $true
      }
    }
  } else {
    $newAll = Get-ChildItem $shotDir -Recurse -Filter 'OU_*.png' -ErrorAction SilentlyContinue |
      Where-Object { -not $before.ContainsKey($_.FullName) }
    # Предпочитаем отложенный кадр (`OU_late_*`): он снят уже в игре, а не на
    # экране создания персонажа (`OU_start_*`).
    $new = $newAll | Where-Object { $_.Name -like 'OU_late_*' } | Sort-Object LastWriteTime -Descending
    if ($new.Count -eq 0) { $new = $newAll | Sort-Object LastWriteTime -Descending }
    if ($new.Count -gt 0) { Copy-Item $new[0].FullName $outShot -Force; $captured = $true }
  }

  if ($captured) { $result.Shot = $outShot; $result.Frame = Measure-Frame $outShot }

  if (-not $KeepOpen -and $proc -and -not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
  if ($srv) { Stop-Process -Id $srv.Id -Force -ErrorAction SilentlyContinue }
  Stop-OuProcs

  # --- Лог ---
  if (-not $RunShipping) { $result.Log = Test-ClientLog $log $WantCats $WantText }

  # --- Итог ---
  $reasons = @()
  if (-not $captured) { $reasons += 'нет кадра' }
  elseif (-not $result.Frame.Ok) { $reasons += ('кадр подозрительный: lit {0:P0}, цветов {1}' -f $result.Frame.Lit, $result.Frame.Unique) }
  if ($RunShipping) {
    if ($exitedEarly) { $reasons += ('процесс упал (код {0})' -f $exitCode) }
  } else {
    if (-not $result.Log.Exists) { $reasons += 'нет лога' }
    else {
      if ($result.Log.MissingCats.Count) { $reasons += ('нет модулей: ' + ($result.Log.MissingCats -join ',')) }
      if ($result.Log.MissingText.Count) { $reasons += ('не сработало: ' + ($result.Log.MissingText -join ' | ')) }
      if ($result.Log.Errors.Count) { $reasons += ('ошибки лога: ' + $result.Log.Errors.Count) }
    }
  }
  $result.Reason = ($reasons -join '; ')
  $result.Ok = ($reasons.Count -eq 0)
  return $result
}

# ============================ основной сценарий ============================

# Собрать список прогонов
$runs = @()
if ($Preset -eq 'all') {
  foreach ($p in $Presets) {
    $runs += [pscustomobject]@{ Name=$p.Name; Flags=$p.Flags; Cats=$p.Cats; Expect=$p.Expect; Wait=[int]$p.Wait }
  }
} elseif ($Preset) {
  $p = $Presets | Where-Object { $_.Name -eq $Preset } | Select-Object -First 1
  if (-not $p) { Write-Host "Неизвестный сценарий '$Preset'. Список: tools/capture.ps1 -List" -ForegroundColor Red; exit 2 }
  $runs += [pscustomobject]@{ Name=$p.Name; Flags=$p.Flags; Cats=$p.Cats; Expect=$p.Expect; Wait=[int]$p.Wait }
} else {
  if (-not $Name) { $Name = 'custom' }
  $runs += [pscustomobject]@{ Name=$Name; Flags=$Flag; Cats=@(); Expect=$Expect; Wait=$WaitSec }
}

Write-Host "== Проверка клиента Open Universe ($([string]::Join(', ', ($runs | ForEach-Object { $_.Name })))) ==" -ForegroundColor Cyan
if ($Shipping) { Write-Host "сборка: упакованная Shipping" -ForegroundColor Yellow }
elseif ($Window) { Write-Host "режим: окно + GDI-кадр" -ForegroundColor Yellow }
else { Write-Host "режим: headless (без окна), кадр от движка" -ForegroundColor Yellow }
Write-Host ""

Stop-OuProcs
trap { Stop-OuProcs; break }

$results = @()
foreach ($run in $runs) {
  Write-Host ("--- {0}: {1}" -f $run.Name, ($run.Flags -join ' ')) -ForegroundColor Cyan
  $res = Invoke-Capture -RunName $run.Name -RunFlags $run.Flags -WantCats $run.Cats `
    -WantText $run.Expect -RunWait $run.Wait -RunWindow:$Window -RunShipping:$Shipping -RunNoServer:$NoServer
  $results += $res

  $status = if ($res.Ok) { 'OK' } else { 'FAIL' }
  $color = if ($res.Ok) { 'Green' } else { 'Red' }
  Write-Host ("  [{0}] {1}" -f $status, $run.Name) -ForegroundColor $color
  if ($res.Shot) { Write-Host ("  кадр: {0}" -f $res.Shot) -ForegroundColor Green }
  if ($res.Frame) { Write-Host ("  графика: {0}x{1}, lit {2:P0}, цветов {3}" -f $res.Frame.Width, $res.Frame.Height, $res.Frame.Lit, $res.Frame.Unique) }
  if ($res.Log -and $res.Log.Exists) { Write-Host ("  лог: {0} строк, ошибок {1}" -f $res.Log.Lines, $res.Log.Errors.Count) }
  if (-not $res.Ok) { Write-Host ("  причина: {0}" -f $res.Reason) -ForegroundColor Red }
  Write-Host ""
}

# --- Итоговая таблица ---
if ($results.Count -gt 1) {
  Write-Host "== Итог: что реализовано ==" -ForegroundColor Cyan
  Write-Host ("  {0,-10} {1,-8} {2}" -f 'фича', 'статус', 'графика (lit / цветов)')
  foreach ($r in $results) {
    $g = if ($r.Frame) { '{0:P0} / {1}' -f $r.Frame.Lit, $r.Frame.Unique } else { '-' }
    $s = if ($r.Ok) { 'OK' } else { 'НЕТ' }
    $color = if ($r.Ok) { 'Green' } else { 'Red' }
    Write-Host ("  {0,-10} {1,-8} {2}" -f $r.Name, $s, $g) -ForegroundColor $color
  }
  $okCount = ($results | Where-Object { $_.Ok }).Count
  Write-Host ""
  Write-Host ("Итого: {0} из {1} сценариев прошли." -f $okCount, $results.Count) -ForegroundColor Yellow
}

$allOk = ($results | Where-Object { -not $_.Ok }).Count -eq 0
if ($allOk) { Write-Host "Проверка клиента: OK" -ForegroundColor Green; exit 0 }
Write-Host "Проверка клиента: есть проблемы (см. выше)" -ForegroundColor Red
exit 1
