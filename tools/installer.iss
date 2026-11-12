; Open Universe — установщик игры (Inno Setup 6).
;
; Собирает OpenUniverse-Setup-<версия>.exe: ставит сервер, клиенты, демо,
; bat-скрипты запускa и создаёт ярлыки в меню «Пуск» и на рабочем столе.
;
; Сборка:
;   ISCC.exe /DMyVersion=0.23.0 tools\installer.iss
;
; Переменные (передаются из release.ps1):
;   MyVersion   — версия (например, 0.23.0)
;   SourceRoot  — корень репозитория

#ifndef MyVersion
  #define MyVersion "0.0.0"
#endif
#ifndef SourceRoot
  #define SourceRoot ".."
#endif

[Setup]
AppId={{7E1F3A2C-9D4B-4E5A-9C77-A1B2C3D4E5F6}
AppName=Open Universe
AppVersion={#MyVersion}
AppVerName=Open Universe {#MyVersion}
AppPublisher=Open Universe
AppPublisherURL=https://andreyshpak2016-lgtm.github.io/open-universe/
AppSupportURL=https://github.com/andreyshpak2016-lgtm/open-universe
AppUpdatesURL=https://github.com/andreyshpak2016-lgtm/open-universe/releases
DefaultDirName={autopf}\Open Universe
DefaultGroupName=Open Universe
DisableProgramGroupPage=yes
OutputBaseFilename=OpenUniverse-Setup-{#MyVersion}
OutputDir={#SourceRoot}\dist
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayName=Open Universe {#MyVersion}
UninstallDisplayIcon={app}\openuniverse.ico
SetupIconFile={#SourceRoot}\assets\openuniverse.ico
LicenseFile={#SourceRoot}\assets\license.txt
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
MinVersion=10.0

[Languages]
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
; Исполняемые файлы: сервер, сетевые клиенты, демо.
Source: "{#SourceRoot}\server\target\release\ou-*.exe"; DestDir: "{app}"; Flags: ignoreversion
; Скрипты запуска (bat + ps1).
Source: "{#SourceRoot}\server.bat"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceRoot}\game.bat"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceRoot}\play.bat"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceRoot}\tools\server.ps1"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "{#SourceRoot}\tools\game.ps1"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "{#SourceRoot}\tools\play.ps1"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "{#SourceRoot}\tools\ou-common.ps1"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "{#SourceRoot}\play.bat"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "{#SourceRoot}\README.md"; DestDir: "{app}"; Flags: ignoreversion
; Значок приложения и ярлыков.
Source: "{#SourceRoot}\assets\openuniverse.ico"; DestDir: "{app}"; Flags: ignoreversion

[Dirs]
; Каталог мира создаём заранее, чтобы сервер сразу его нашёл.
Name: "{app}\world"

[Icons]
Name: "{group}\Open Universe — сервер"; Filename: "{app}\server.bat"; WorkingDir: "{app}"; IconFilename: "{app}\openuniverse.ico"
Name: "{group}\Open Universe — игра (UE5)"; Filename: "{app}\game.bat"; WorkingDir: "{app}"; IconFilename: "{app}\openuniverse.ico"
Name: "{group}\Быстрый запуск (сервер + игроки)"; Filename: "{app}\play.bat"; WorkingDir: "{app}"; IconFilename: "{app}\openuniverse.ico"
Name: "{group}\Документация"; Filename: "{app}\README.md"
Name: "{group}\Удалить Open Universe"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Open Universe"; Filename: "{app}\play.bat"; WorkingDir: "{app}"; Tasks: desktopicon; IconFilename: "{app}\openuniverse.ico"

[Run]
Filename: "{app}\play.bat"; Description: "Запустить игру сейчас"; Flags: nowait postinstall skipifsilent
