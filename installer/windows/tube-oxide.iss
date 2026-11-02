; Inno Setup Script for Tube-oxide
#ifndef AppVersion
  #define AppVersion "0.99.14"
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
SetupIconFile=..\..\assets\icon.ico
UninstallDisplayIcon={app}\tube-oxide.exe
CloseApplications=yes
CloseApplicationsFilter=tube-oxide.exe
RestartApplications=no
ArchitecturesInstallIn64BitMode=x64compatible
ArchitecturesAllowed=x64compatible
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ChangesAssociations=yes

[Languages]
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "startmenuicon"; Description: "Crear acceso directo en el Menú Inicio"; GroupDescription: "{cm:AdditionalIcons}"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "assocyoutube"; Description: "Asociar enlaces y protocolos de YouTube (vnd.youtube://, youtube://)"; GroupDescription: "Asociaciones de sistema:"

[Files]
Source: "..\..\pkg\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\..\assets\icon.ico"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Tube-oxide"; Filename: "{app}\tube-oxide.exe"; IconFilename: "{app}\icon.ico"; Tasks: startmenuicon
Name: "{autoprograms}\Desinstalar Tube-oxide"; Filename: "{uninstallexe}"; Tasks: startmenuicon
Name: "{autodesktop}\Tube-oxide"; Filename: "{app}\tube-oxide.exe"; IconFilename: "{app}\icon.ico"; Tasks: desktopicon

[Registry]
; Asociar protocolo de YouTube vnd.youtube (abierto por navegadores y extensiones)
Root: HKA; Subkey: "Software\Classes\vnd.youtube"; ValueType: string; ValueName: ""; ValueData: "URL:YouTube Protocol"; Flags: uninsdeletekey; Tasks: assocyoutube
Root: HKA; Subkey: "Software\Classes\vnd.youtube"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""; Flags: uninsdeletekey; Tasks: assocyoutube
Root: HKA; Subkey: "Software\Classes\vnd.youtube\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\tube-oxide.exe,0"; Flags: uninsdeletekey; Tasks: assocyoutube
Root: HKA; Subkey: "Software\Classes\vnd.youtube\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\tube-oxide.exe"" ""%1"""; Flags: uninsdeletekey; Tasks: assocyoutube

; Asociar protocolo youtube://
Root: HKA; Subkey: "Software\Classes\youtube"; ValueType: string; ValueName: ""; ValueData: "URL:YouTube Protocol"; Flags: uninsdeletekey; Tasks: assocyoutube
Root: HKA; Subkey: "Software\Classes\youtube"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""; Flags: uninsdeletekey; Tasks: assocyoutube
Root: HKA; Subkey: "Software\Classes\youtube\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\tube-oxide.exe,0"; Flags: uninsdeletekey; Tasks: assocyoutube
Root: HKA; Subkey: "Software\Classes\youtube\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\tube-oxide.exe"" ""%1"""; Flags: uninsdeletekey; Tasks: assocyoutube

; Registrar Capabilities para que Windows lo reconozca como reproductor / navegador de YouTube
Root: HKA; Subkey: "Software\Tube-oxide\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "Cliente nativo y ligero de YouTube en Rust"; Flags: uninsdeletekey; Tasks: assocyoutube
Root: HKA; Subkey: "Software\Tube-oxide\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\tube-oxide.exe,0"; Flags: uninsdeletekey; Tasks: assocyoutube
Root: HKA; Subkey: "Software\Tube-oxide\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "Tube-oxide"; Flags: uninsdeletekey; Tasks: assocyoutube
Root: HKA; Subkey: "Software\Tube-oxide\Capabilities\UrlAssociations"; ValueType: string; ValueName: "vnd.youtube"; ValueData: "vnd.youtube"; Flags: uninsdeletekey; Tasks: assocyoutube
Root: HKA; Subkey: "Software\Tube-oxide\Capabilities\UrlAssociations"; ValueType: string; ValueName: "youtube"; ValueData: "youtube"; Flags: uninsdeletekey; Tasks: assocyoutube
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "Tube-oxide"; ValueData: "Software\Tube-oxide\Capabilities"; Flags: uninsdeletevalue; Tasks: assocyoutube

[Run]
Filename: "{app}\tube-oxide.exe"; Description: "{cm:LaunchProgram,Tube-oxide}"; Flags: nowait postinstall skipifsilent
