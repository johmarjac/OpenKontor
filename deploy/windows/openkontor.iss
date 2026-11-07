; Windows installer for OpenKontor (Inno Setup 6).
; Built by the release workflow: iscc /DVersion=0.1.0 /DArch=x64 /DSource=dist\OpenKontor-0.1.0-windows-x64 openkontor.iss

#ifndef Version
  #define Version "0.0.0"
#endif
#ifndef Arch
  #define Arch "x64"
#endif
#ifndef Source
  #define Source "."
#endif

[Setup]
AppId={{7E2B9C4A-3F1D-4C8E-9A57-0B6D2E1F4A83}
AppName=OpenKontor
AppVersion={#Version}
AppPublisher=OpenKontor
AppPublisherURL=https://github.com/johmarjac/OpenKontor
DefaultDirName={autopf}\OpenKontor
DefaultGroupName=OpenKontor
DisableProgramGroupPage=yes
LicenseFile={#Source}\LICENSE
OutputBaseFilename=OpenKontor-{#Version}-windows-{#Arch}-Setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
PrivilegesRequiredOverridesAllowed=dialog
#if Arch == "arm64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif

[Languages]
Name: "de"; MessagesFile: "compiler:Languages\German.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#Source}\openkontor.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\openkontor-server.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\SERVER-INSTALLATION.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\OpenKontor"; Filename: "{app}\openkontor.exe"
Name: "{autodesktop}\OpenKontor"; Filename: "{app}\openkontor.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\openkontor.exe"; Description: "{cm:LaunchProgram,OpenKontor}"; Flags: nowait postinstall skipifsilent
