#define MyAppName "THzip-SpellArchive"
#define MyAppVersion "0.2.0"
#define MyAppExeName "thzip-spellarchive.exe"

[Setup]
AppId={{6C7A2E5A-6F5B-4D5A-9E36-2B7A1F8C4D10}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher=THzip-SpellArchive contributors
DefaultDirName={localappdata}\Programs\THzip-SpellArchive
DefaultGroupName=THzip-SpellArchive
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=THzip-SpellArchive-{#MyAppVersion}-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayIcon={app}\{#MyAppExeName}

[InstallDelete]
Type: files; Name: "{app}\spell_archive.exe"

[Files]
Source: "..\dist\THzip-SpellArchive\*"; DestDir: "{app}"; Flags: recursesubdirs ignoreversion

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "创建桌面快捷方式"; GroupDescription: "附加任务:"

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "启动 {#MyAppName}"; Flags: nowait postinstall skipifsilent
