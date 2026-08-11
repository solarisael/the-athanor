#ifndef Version
  #error Version must be supplied with /DVersion=x.y.z
#endif
#ifndef SourceRoot
  #define SourceRoot "..\dist"
#endif

[Setup]
AppId={{D201D245-E72A-4EE5-B13C-7E5BC6312CD0}
AppName=The Athanor
AppVersion={#Version}
AppPublisher=Solarisael
DefaultDirName={autopf}\Solarisael\Athanor
DefaultGroupName=The Athanor
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
OutputDir={#SourceRoot}
OutputBaseFilename=The-Athanor-{#Version}-windows-x64
UninstallDisplayIcon={app}\bin\athanor-manage.exe
DisableProgramGroupPage=yes

[Files]
Source: "{#SourceRoot}\payload\bin\athanor-manage.exe"; DestDir: "{app}\bin"; Flags: ignoreversion
Source: "{#SourceRoot}\payload\*"; DestDir: "{tmp}\athanor-payload"; Flags: recursesubdirs createallsubdirs deleteafterinstall

[Run]
Filename: "{app}\bin\athanor-manage.exe"; Parameters: "install --staging ""{tmp}\athanor-payload"" --manifest ""{tmp}\athanor-payload\release-manifest.json"" {code:ExternalDatabaseArgument}"; StatusMsg: "Verifying and starting the managed Athanor runtime..."; Flags: runhidden waituntilterminated

[UninstallRun]
Filename: "{app}\bin\athanor-manage.exe"; Parameters: "uninstall"; RunOnceId: "AthanorPreserveDataUninstall"; Flags: runhidden waituntilterminated

[Icons]
Name: "{group}\The Athanor"; Filename: "{app}\versions\{#Version}\bin\athanor-gui.exe"; Parameters: "--path ""{app}\versions\{#Version}\runtime\godot"""; WorkingDir: "{app}\versions\{#Version}\runtime\godot"
Name: "{group}\Athanor Doctor"; Filename: "{app}\bin\athanor-manage.exe"; Parameters: "doctor"

[Code]
function ExternalDatabaseArgument(Param: String): String;
var
  ConfigFile: String;
begin
  ConfigFile := ExpandConstant('{param:EXTERNALDATABASEFILE|}');
  if ConfigFile = '' then
    Result := ''
  else begin
    if not FileExists(ConfigFile) then
      RaiseException('EXTERNALDATABASEFILE does not exist: ' + ConfigFile);
    Result := '--external-database-file "' + ConfigFile + '"';
  end;
end;
