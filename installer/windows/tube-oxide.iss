; Inno Setup Script for Tube-oxide
#ifndef AppVersion
  #define AppVersion "0.99.7"
#endif

[Setup]
AppId={{D37B47C5-81E2-4467-9FA0-A6E55A94C777}}
AppName=Tube-oxide
AppVersion={#AppVersion}
AppVerName=Tube-oxide {#AppVersion}
AppPublisher=OxideTube
DefaultDirName={autopf}\Tube-oxide
DefaultGroupName=Tube-oxide
DisableProgramGroupPage=yes
OutputBaseFilename=tube-oxide-windows-x86_64-installer-{#AppVersion}
OutputDir=dist
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
SetupIconFile=assets\icon.ico
UninstallDisplayIcon={app}\tube-oxide.exe
CloseApplications=yes
CloseApplicationsFilter=tube-oxide.exe
RestartApplications=no
ArchitecturesInstallIn64BitMode=x64compatible
ArchitecturesAllowed=x64compatible
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog

[Languages]
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: checkedonce
Name: "taskbaricon"; Description: "Crear acceso directo en la barra de tareas"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "pkg\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "assets\icon.ico"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Tube-oxide"; Filename: "{app}\tube-oxide.exe"; IconFilename: "{app}\icon.ico"
Name: "{autoprograms}\Desinstalar Tube-oxide"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Tube-oxide"; Filename: "{app}\tube-oxide.exe"; IconFilename: "{app}\icon.ico"; Tasks: desktopicon
Name: "{userappdata}\Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar\Tube-oxide"; Filename: "{app}\tube-oxide.exe"; IconFilename: "{app}\icon.ico"; Tasks: taskbaricon

[Run]
Filename: "{app}\tube-oxide.exe"; Description: "{cm:LaunchProgram,Tube-oxide}"; Flags: nowait postinstall skipifsilent
