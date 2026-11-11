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
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
MinVersion=10.0
DiskSpanning=no

[Languages]
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Dirs]
Name: "{app}\server\world"

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

[Icons]
; Лаунчер — главный ярлык: вход, новости, автообновление, запуск игры.
Name: "{group}\Open Universe — лаунчер"; Filename: "{app}\OpenUniverseLauncher.exe"
Name: "{group}\Open Universe (напрямую)"; Filename: "{app}\OpenUniverse.exe"
Name: "{group}\Сервер (для игры по сети)"; Filename: "{app}\server\server.bat"; WorkingDir: "{app}\server"
Name: "{group}\Удалить Open Universe"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Open Universe"; Filename: "{app}\OpenUniverseLauncher.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\OpenUniverseLauncher.exe"; Description: "Запустить лаунчер Open Universe"; Flags: nowait postinstall skipifsilent
