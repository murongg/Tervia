; AppId must remain stable across versions so upgrades reuse the same installation.
[Setup]
AppId=dev.tervia.desktop
AppName=Tervia
AppVersion={#AppVersion}
VersionInfoVersion={#AppNumericVersion}.0
AppPublisher=Tervia contributors
AppPublisherURL=https://github.com/murongg/Tervia
DefaultDirName={localappdata}\Programs\Tervia
DefaultGroupName=Tervia
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
OutputDir=..\build
OutputBaseFilename=Tervia-windows-x64-setup
SetupIconFile=..\assets\tervia.ico
UninstallDisplayIcon={app}\Tervia.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "..\build\windows\Tervia\*"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Tervia"; Filename: "{app}\Tervia.exe"
Name: "{autodesktop}\Tervia"; Filename: "{app}\Tervia.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\Tervia.exe"; Description: "Launch Tervia"; Flags: nowait postinstall skipifsilent
