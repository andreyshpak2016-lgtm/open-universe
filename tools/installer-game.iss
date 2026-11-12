; Open Universe — установщик ГРАФИЧЕСКОЙ игры (Inno Setup 6).
;
; Ставит полностью самостоятельную игру: клиент UE5 (OpenUniverse.exe),
; сервер и поддержку запуска. Unreal Engine устанавливать НЕ нужно.
;
; Сборка:
;   ISCC.exe /DMyVersion=0.24.0 /DSourceRoot=.. tools\installer-game.iss
;
; Ожидает готовую сборку в client\Packaged\Windows\OpenUniverse\.

#ifndef MyVersion
  #define MyVersion "0.0.0"
#endif
#ifndef SourceRoot
  #define SourceRoot ".."
#endif

[Setup]
AppId={{8F2B4D1E-6A3C-4B7D-9E12-OUGAME0000001}
AppName=Open Universe
AppVersion={#MyVersion}
AppVerName=Open Universe {#MyVersion}
AppPublisher=Open Universe
AppPublisherURL=https://andreyshpak2016-lgtm.github.io/open-universe/
DefaultDirName={autopf}\Open Universe
DefaultGroupName=Open Universe
DisableProgramGroupPage=yes
OutputBaseFilename=OpenUniverse-Game-Setup-{#MyVersion}
OutputDir={#SourceRoot}\dist
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayName=Open Universe {#MyVersion}
UninstallDisplayIcon={app}\openuniverse.ico
SetupIconFile={#SourceRoot}\assets\openuniverse.ico
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
MinVersion=10.0
DiskSpanning=no
; Обновление поверх работающей игры: закрыть запущенные файлы и заменить их.
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Dirs]
Name: "{app}\server\world"

[InstallDelete]
; Убираем ярлыки от старых версий (у них были другие имена).
Type: files; Name: "{group}\Open Universe — лаунчер.lnk"
Type: files; Name: "{group}\Open Universe (напрямую).lnk"
Type: files; Name: "{group}\Open Universe — игра (UE5).lnk"
Type: files; Name: "{group}\Сервер (для игры по сети).lnk"
Type: files; Name: "{group}\Open Universe — сервер.lnk"
Type: files; Name: "{group}\Быстрый запуск (сервер + игроки).lnk"

[Files]
; Вся собранная игра: лаунчер OpenUniverse.exe, папки OpenUniverse\ и Engine\.
Source: "{#SourceRoot}\client\Packaged\Windows\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
; Сервер и демо-утилиты рядом (для игры по сети).
Source: "{#SourceRoot}\server\target\release\ou-*.exe"; DestDir: "{app}\server"; Flags: ignoreversion
Source: "{#SourceRoot}\tools\server.ps1"; DestDir: "{app}\server"; Flags: ignoreversion
Source: "{#SourceRoot}\tools\ou-common.ps1"; DestDir: "{app}\server"; Flags: ignoreversion
Source: "{#SourceRoot}\server.bat"; DestDir: "{app}\server"; Flags: ignoreversion
Source: "{#SourceRoot}\README.md"; DestDir: "{app}"; Flags: ignoreversion
; Версия — чтобы лаунчер понимал, что установлено.
Source: "{#SourceRoot}\VERSION"; DestDir: "{app}"; DestName: "VERSION"; Flags: ignoreversion
; Лаунчер: вход, новости, автообновление.
Source: "{#SourceRoot}\launcher\OpenUniverseLauncher.exe"; DestDir: "{app}"; Flags: ignoreversion
; Баннер для шапки лаунчера.
Source: "{#SourceRoot}\launcher\banner.jpg"; DestDir: "{app}"; Flags: ignoreversion
; Значок приложения и ярлыков.
Source: "{#SourceRoot}\assets\openuniverse.ico"; DestDir: "{app}"; Flags: ignoreversion
; Иконка самой игры (лежит рядом с игрой; используется ярлыком).
Source: "{#SourceRoot}\assets\openuniverse.ico"; DestDir: "{app}\OpenUniverse"; DestName: "OpenUniverse.ico"; Flags: ignoreversion

[Icons]
; Лаунчер — главный ярлык: вход, новости, автообновление, запуск игры.
Name: "{group}\Open Universe"; Filename: "{app}\OpenUniverseLauncher.exe"; IconFilename: "{app}\openuniverse.ico"; Comment: "Лаунчер Open Universe"
Name: "{group}\Играть"; Filename: "{app}\OpenUniverse.exe"; IconFilename: "{app}\OpenUniverse\OpenUniverse.ico"
Name: "{group}\Сервер (по сети)"; Filename: "{app}\server\server.bat"; WorkingDir: "{app}\server"; IconFilename: "{app}\openuniverse.ico"
Name: "{group}\Удалить Open Universe"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Open Universe"; Filename: "{app}\OpenUniverseLauncher.exe"; Tasks: desktopicon; IconFilename: "{app}\openuniverse.ico"; Comment: "Open Universe — лаунчер"

[Run]
; При обычной установке предлагаем запустить лаунчер. При тихой (её делает
; сам лаунчер) — пропускаем: лаунчер уже открыт и остаётся работать.
Filename: "{app}\OpenUniverseLauncher.exe"; Description: "Запустить лаунчер Open Universe"; Flags: nowait postinstall skipifsilent
