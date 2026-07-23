; Inno Setup script for COM Port Plotter (RKM / RKMS)
; Build via: .\installer\build-installer.ps1
;
; Requires: Inno Setup 6+ (https://jrsoftware.org/isinfo.php)
;
; IMPORTANT: shortcut / folder names must NOT contain \/:*?"<>|
; (Windows path rules — otherwise IPersistFile::Save 0x80070003)
;
; Prolific COM driver: place official installer at
;   drivers\prolific\PL23XX_Prolific_DriverInstaller.exe
; Silent flag per Prolific release notes: /s

#define MyAppName "COM Port Plotter"
#define MyAppShortcutName "COM Port Plotter (RKM)"
#define MyAppDisplayName "COM Port Plotter (RKM-RCMS)"
#define MyAppVersion "0.1.0"
#define MyAppPublisher "RPG"
#define MyAppExeName "com-port-plotter.exe"
#define MyAppId "{{A7C3E9F2-4B1D-4E8A-9C2F-6D5E8A1B3C4D}"
#define ProlificInstaller "PL23XX_Prolific_DriverInstaller.exe"

[Setup]
AppId={#MyAppId}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppDisplayName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
OutputDir=output
OutputBaseFilename=COMPortPlotter-Setup-{#MyAppVersion}
SetupIconFile=..\assets\rheogram.ico
Compression=lzma
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
UninstallDisplayIcon={app}\{#MyAppExeName}
VersionInfoVersion={#MyAppVersion}.0
VersionInfoCompany={#MyAppPublisher}
VersionInfoProductName={#MyAppDisplayName}
AllowNoIcons=yes

[Languages]
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "prolificdriver"; Description: "Install Prolific USB-COM driver (PL2303 / PL23XX)"; GroupDescription: "Device drivers:"; Flags: checkedonce

[Files]
Source: "..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "redist\VC_redist.x64.exe"; DestDir: "{tmp}"; Flags: deleteafterinstall skipifsourcedoesntexist
; Official Prolific setup (bundled if present at build time)
Source: "drivers\prolific\{#ProlificInstaller}"; DestDir: "{tmp}"; Flags: deleteafterinstall skipifsourcedoesntexist

[Icons]
Name: "{group}\{#MyAppShortcutName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; IconFilename: "{app}\{#MyAppExeName}"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppShortcutName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; IconFilename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{tmp}\VC_redist.x64.exe"; \
  Parameters: "/install /quiet /norestart"; \
  StatusMsg: "Installing Microsoft Visual C++ Redistributable..."; \
  Flags: waituntilterminated skipifdoesntexist
; Prolific silent install (/s). Prefer unplugging the USB-COM adapter first.
Filename: "{tmp}\{#ProlificInstaller}"; \
  Parameters: "/s"; \
  StatusMsg: "Installing Prolific COM port driver..."; \
  Flags: waituntilterminated skipifdoesntexist; \
  Tasks: prolificdriver
Filename: "{app}\{#MyAppExeName}"; \
  Description: "{cm:LaunchProgram,{#MyAppDisplayName}}"; \
  Flags: nowait postinstall skipifsilent
