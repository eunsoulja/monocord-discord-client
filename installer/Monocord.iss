; Inno Setup script for Monocord — compile with ISCC.exe (Inno Setup 6).
; Produces a per-user installer (no admin required):
;   "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" installer\Monocord.iss
;
; The app requires the WebView2 Runtime (preinstalled on Windows 10/11).
; For a dependency-free install without Inno, use installer\install.ps1.

#define AppName "Monocord"
#define AppVersion "0.1.0"
#define AppExe "Monocord.exe"

[Setup]
AppId={{8F3E2C1A-4B5D-4E6F-9A0B-1C2D3E4F5A6B}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=eunsoulja
DefaultDirName={localappdata}\Programs\{#AppName}
DefaultGroupName={#AppName}
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=Monocord-Setup-{#AppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "..\target\release\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{group}\Uninstall {#AppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Additional icons:"

[Run]
Filename: "{app}\{#AppExe}"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent
