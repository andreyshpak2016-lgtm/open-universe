# Прогон проверок Open Universe на ДРУГОМ компьютере по SSH.
#
# Зачем: тестировать на второй машине, не трогая мышь этого компьютера. Скрипт
# передаёт по сети свежий код (git bundle), разворачивает его на удалённом ПК и
# запускает `tools/test-all.ps1`, затем печатает сводку здесь.
#
# Использование:
#   powershell -ExecutionPolicy Bypass -File tools/test-remote.ps1 -Host 192.168.0.12
#   powershell -File tools/test-remote.ps1 -Host 192.168.0.12 -User fisk- -SkipUE
#   powershell -File tools/test-remote.ps1 -Host 192.168.0.12 -RemoteDir C:\ou -EngineRoot "C:\Program Files\Epic Games\UE_5.5"
#
# Предпосылки (один раз):
#   1. На удалённом ПК запущен OpenSSH-сервер (`sshd`).
#   2. Ваш публичный ключ добавлен в `%USERPROFILE%\.ssh\authorized_keys` на нём.
#   Тогда вход по ключу — без пароля и без мыши.

param(
  [Parameter(Mandatory = $true)][Alias("Host")][string]$Target,
  [string]$User = $env:USERNAME,
  [int]$Port = 22,
  [string]$RemoteDir = 'D:\ou-remote',
  [switch]$SkipUE,
  [string]$EngineRoot = "",
  [switch]$KeepBundle
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$SshTarget = "$User@$Target"
$Ssh = @('-o', 'BatchMode=yes', '-o', 'ConnectTimeout=10',
         '-o', 'StrictHostKeyChecking=accept-new', '-p', "$Port")
$Scp = @('-o', 'BatchMode=yes', '-o', 'StrictHostKeyChecking=accept-new', '-P', "$Port")

function Remote([string]$Cmd) {
  # Не роняем скрипт при ненулевом коде ssh — вернём вывод как текст.
  # Пути инструментов (cargo/git) добавляются внутри самих удалённых скриптов,
  # т.к. PATH свежей SSH-сессии может их не знать.
  $prev = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  try {
    return ((& ssh @Ssh $SshTarget $Cmd 2>&1) -join "`n")
  } finally {
    $ErrorActionPreference = $prev
  }
}

Write-Host "== Удалённые проверки Open Universe ==" -ForegroundColor Cyan
Write-Host "хост: $SshTarget : $Port, каталог: $RemoteDir" -ForegroundColor DarkGray

# --- 1. Связь и вход по ключу ----------------------------------------------
Write-Host "[1/5] Проверка SSH-доступа..." -ForegroundColor Cyan
$hello = Remote 'echo OU_REMOTE_OK && hostname'
if ($hello -notmatch 'OU_REMOTE_OK') {
  $pub = Get-Content "$env:USERPROFILE\.ssh\id_ed25519.pub" -ErrorAction SilentlyContinue
  Write-Host "НЕ УДАЛОСЬ войти по SSH:" -ForegroundColor Red
  Write-Host ($hello.Trim()) -ForegroundColor DarkGray
  Write-Host ""
  Write-Host "Как починить (на УДАЛЁННОМ ПК, один раз; мышь этого ПК не нужна):" -ForegroundColor Yellow
  Write-Host "  Откройте на нём PowerShell ОТ АДМИНИСТРАТОРА и выполните:" -ForegroundColor Yellow
  Write-Host "    Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0" -ForegroundColor Yellow
  Write-Host "    Start-Service sshd" -ForegroundColor Yellow
  Write-Host "    Set-Service -Name sshd -StartupType Automatic" -ForegroundColor Yellow
  Write-Host "    New-NetFirewallRule -Name sshd -DisplayName OpenSSH -Enabled True -Direction Inbound -Protocol TCP -Action Allow -LocalPort 22" -ForegroundColor Yellow
  Write-Host ""
  Write-Host "  Затем добавьте мой публичный ключ (чтобы зайти без пароля):" -ForegroundColor Yellow
  Write-Host "    mkdir `"`$env:USERPROFILE\.ssh`" -Force" -ForegroundColor Yellow
  Write-Host "    Add-Content -Path `"`$env:USERPROFILE\.ssh\authorized_keys`" -Value `"$pub`"" -ForegroundColor Yellow
  Write-Host ""
  Write-Host "Если sshd уже стоит — достаточно добавить ключ выше." -ForegroundColor Yellow
  exit 2
}
Write-Host ("      OK: " + ($hello -replace "`n", ' ').Trim()) -ForegroundColor Green

# --- 2. Инвентарь окружения удалённого ПК ----------------------------------
Write-Host "[2/5] Инвентарь удалённого ПК..." -ForegroundColor Cyan
$invScript = Join-Path $env:TEMP 'ou-inventory.ps1'
@'
# В свежеустановленных инструментах PATH SSH-сессии может их не видеть —
# добавляем явно (cargo/rustup и git for windows).
$env:PATH = "$env:USERPROFILE\.cargo\bin;C:\Program Files\Git\cmd;C:\Program Files\Git\bin;" + $env:PATH
$has = @()
if (Get-Command cargo -ErrorAction SilentlyContinue) { $has += 'cargo' }
if (Get-Command git -ErrorAction SilentlyContinue)   { $has += 'git' }
$vs = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (Test-Path $vs) { $has += 'msvc' }
foreach ($b in @('D:\UE_5.5','C:\Program Files\Epic Games\UE_5.5','D:\Epic Games\UE_5.5','E:\UE_5.5')) {
  if (Test-Path (Join-Path $b 'Engine\Binaries\Win64\UnrealEditor.exe')) { $has += 'ue'; break }
}
Write-Output ("OU_INV:" + ($has -join ','))
'@ | Set-Content $invScript -Encoding UTF8
& scp @Scp $invScript "${SshTarget}:$RemoteDir-inventory.ps1" 2>&1 | Out-Null
$inv = Remote "powershell -NoProfile -ExecutionPolicy Bypass -File `"$RemoteDir-inventory.ps1`""
$invLine = ([regex]'OU_INV:(.*)').Match($inv).Groups[1].Value.Trim()
Write-Host "      установлено: $invLine" -ForegroundColor DarkGray
if ($invLine -notmatch 'cargo') { Write-Host "      НЕТ Rust (cargo) — тесты ядра не пойдут." -ForegroundColor Yellow }
if ($invLine -notmatch 'git')   { Write-Host "      НЕТ git — установите: winget install Git.Git" -ForegroundColor Yellow }

# --- 3. Передача свежего кода ----------------------------------------------
Write-Host "[3/5] Передаю свежий код (git bundle)..." -ForegroundColor Cyan
$bundle = Join-Path $env:TEMP 'ou-remote.bundle'
& git -C $root bundle create $bundle --all 2>&1 | Select-Object -Last 1 | ForEach-Object { Write-Host "      $_" -ForegroundColor DarkGray }
if (-not (Test-Path $bundle)) { Write-Host "Не создался bundle." -ForegroundColor Red; exit 3 }

Remote "powershell -NoProfile -Command `"New-Item -ItemType Directory -Force -Path '$RemoteDir' | Out-Null`"" | Out-Null
& scp @Scp $bundle "${SshTarget}:$RemoteDir/ou.bundle" 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) { Write-Host "scp bundle не прошёл." -ForegroundColor Red; exit 3 }
Write-Host "      доставлен" -ForegroundColor Green

# --- 4. Развернуть код на удалённом ПК -------------------------------------
Write-Host "[4/5] Разворачиваю репозиторий..." -ForegroundColor Cyan
$deployScript = Join-Path $env:TEMP 'ou-deploy.ps1'
@"
`$ErrorActionPreference = 'Continue'
# Пути инструментов (в SSH-сессии PATH может быть устаревшим).
`$env:PATH = "`$env:USERPROFILE\.cargo\bin;C:\Program Files\Git\cmd;C:\Program Files\Git\bin;" + `$env:PATH
`$dir = '$RemoteDir\open-universe'
if (-not (Test-Path "`$dir\.git")) {
  git clone '$RemoteDir\ou.bundle' "`$dir" 2>&1 | Out-Null
  git -C "`$dir" remote set-url origin https://github.com/andreyshpak2016-lgtm/open-universe.git 2>&1 | Out-Null
} else {
  git -C "`$dir" fetch '$RemoteDir\ou.bundle' 'refs/heads/*:refs/remotes/bundle/*' --force 2>&1 | Out-Null
  git -C "`$dir" checkout -f main 2>&1 | Out-Null
  git -C "`$dir" reset --hard bundle/main 2>&1 | Out-Null
}
Write-Output ('OU_HEAD:' + (git -C "`$dir" rev-parse --short HEAD))
"@ | Set-Content $deployScript -Encoding UTF8
& scp @Scp $deployScript "${SshTarget}:$RemoteDir-deploy.ps1" 2>&1 | Out-Null
$head = Remote "powershell -NoProfile -ExecutionPolicy Bypass -File `"$RemoteDir-deploy.ps1`""
$headLine = ([regex]'OU_HEAD:(.*)').Match($head).Groups[1].Value.Trim()
Write-Host "      развёрнут HEAD: $headLine" -ForegroundColor Green
if (-not $headLine) { Write-Host ($head.Trim()) -ForegroundColor Yellow }

# --- 5. Прогон тестов на удалённом ПК --------------------------------------
Write-Host "[5/5] Запускаю tools/test-all.ps1 на удалённом ПК (может занять время)..." -ForegroundColor Cyan
$runScript = Join-Path $env:TEMP 'ou-run.ps1'
$skip = if ($SkipUE) { ' -SkipUE' } else { '' }
$headless = if ($SkipUE) { '' } else { ' -Headless' }
$eng = if ($EngineRoot) { " -EngineRoot `"$EngineRoot`"" } else { '' }
# Скриншоты: если не SkipUE — после тестов снимем кадры ключевых сценариев через
# tools/capture.ps1. По SSH процесс идёт в сессии 0 (без видимого рабочего стола),
# поэтому окно недоступно — снимаем в headless-режиме (`-RenderOffscreen`), UE сам
# пишет PNG в Saved\Screenshots (мышь и рабочий стол не нужны).
$capNames = if ($SkipUE) { '' } else { "'orbit1','orbit2','orbit3','orbit0','belt','menus','voxels'" }
# Пересборка UE-модуля: правки клиента (C++) попадают в код, но не в бинарник,
# пока его не пересоберут. Чтобы кадры были СВЕЖИМИ, собираем модуль перед
# съёмкой (при -SkipUE не нужно — там UE не используется).
$engForBuild = if ($EngineRoot) { $EngineRoot } else { 'D:\UE_5.5' }
$rebuild = if ($SkipUE) { '' } else {
  "Write-Host '=== rebuild UE module ==='`npowershell -ExecutionPolicy Bypass -File tools/build-client.ps1 -Build -EngineRoot `"$engForBuild`""
}
@"
`$ErrorActionPreference = 'Continue'
`$env:PATH = "`$env:USERPROFILE\.cargo\bin;C:\Program Files\Git\cmd;C:\Program Files\Git\bin;" + `$env:PATH
Set-Location '$RemoteDir\open-universe'
$rebuild
powershell -ExecutionPolicy Bypass -File tools/test-all.ps1$skip$headless$eng
`$rc = `$LASTEXITCODE
`$caps = @($capNames)
if (`$caps.Count -gt 0) {
  foreach (`$p in `$caps) {
    powershell -ExecutionPolicy Bypass -File tools/capture.ps1 -Preset `$p -Dx11
  }
  # Кадры фича-сценария (смешанные воксели, ядра, коллизия) — headless + DX11.
  powershell -ExecutionPolicy Bypass -File tools/capture.ps1 -Flag '-OUAutoConnect -OUMix -OUCore -OUCoreKinds -OUWalkInto -OUGhostLog' -Name features -Dx11
}
Write-Output ('OU_EXIT:' + `$rc)
"@ | Set-Content $runScript -Encoding UTF8
& scp @Scp $runScript ("{0}:{1}-run.ps1" -f $SshTarget, ($RemoteDir -replace '\\', '/')) 2>&1 | Out-Null
$runPath = ($RemoteDir -replace '\\', '/') + '-run.ps1'
$report = Remote "powershell -NoProfile -ExecutionPolicy Bypass -File $runPath"
Write-Host $report
$m = ([regex]'OU_EXIT:(\d+)').Match($report)
$exitCode = if ($m.Success) { [int]$m.Groups[1].Value } else { 1 }

# --- 6. Забрать скриншоты с удалённого ПК ----------------------------------
$shotsLocal = Join-Path $root 'shots\remote'
New-Item -ItemType Directory -Force -Path $shotsLocal | Out-Null
Write-Host "[+] Забираю скриншоты с удалённого ПК..." -ForegroundColor Cyan
# Имя пользователя может быть кириллическим — `scp` при вытягивании спотыкается
# о разбор аргумента. Надёжнее упаковать каталог в tar на удалённой стороне и
# распаковать здесь (через cmd /c с редиректом в файл — бинарный поток).
$tarLocal = Join-Path $env:TEMP 'ou-shots.tar'
$remoteShotsDir = $RemoteDir + '\open-universe'
$prev = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
try {
  Remove-Item $tarLocal -ErrorAction SilentlyContinue
  cmd /c "ssh -o BatchMode=yes -o ConnectTimeout=10 ""$SshTarget"" ""cd /d $remoteShotsDir && tar -cf - shots"" > ""$tarLocal"" 2>nul"
  if ((Test-Path $tarLocal) -and (Get-Item $tarLocal).Length -gt 0) {
    Push-Location $shotsLocal
    & tar -xf $tarLocal 2>$null
    Pop-Location
  }
} finally { $ErrorActionPreference = $prev }
$got = @(Get-ChildItem $shotsLocal -Recurse -Filter '*.png' -ErrorAction SilentlyContinue)
if ($got.Count -gt 0) {
  Write-Host ("      получено кадров: " + $got.Count) -ForegroundColor Green
  $got | Select-Object -Last 8 | ForEach-Object { Write-Host ("      " + $_.Name) -ForegroundColor DarkGray }
} else {
  Write-Host "      кадров не получено (на удалённом ПК нет UE — снимите на этом)" -ForegroundColor Yellow
}
Remove-Item $tarLocal -ErrorAction SilentlyContinue

Remove-Item $invScript, $deployScript, $runScript -ErrorAction SilentlyContinue
if (-not $KeepBundle) { Remove-Item $bundle -ErrorAction SilentlyContinue }

Write-Host ""
if ($exitCode -eq 0) { Write-Host "Удалённый прогон: ВСЁ ЗЕЛЁНОЕ" -ForegroundColor Green }
else { Write-Host "Удалённый прогон: ЕСТЬ ОШИБКИ (код $exitCode)" -ForegroundColor Red }
exit $exitCode
