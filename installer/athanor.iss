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
Source: "{#SourceRoot}\payload\bin\athanor.exe"; DestDir: "{app}\bin"; Flags: ignoreversion
Source: "{#SourceRoot}\payload\bin\athanor-manage.exe"; DestDir: "{app}\bin"; Flags: ignoreversion
Source: "{#SourceRoot}\payload\bin\athanor-omp-loader.ts"; DestDir: "{app}\bin"; Flags: ignoreversion
Source: "{#SourceRoot}\payload\*"; DestDir: "{tmp}\athanor-payload"; Flags: recursesubdirs createallsubdirs deleteafterinstall


[UninstallRun]
Filename: "{app}\bin\athanor-manage.exe"; Parameters: "uninstall"; RunOnceId: "AthanorPreserveDataUninstall"; Flags: runhidden waituntilterminated

[Icons]
Name: "{group}\The Athanor"; Filename: "{app}\bin\athanor.exe"; WorkingDir: "{app}\bin"
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

function HouseConfigArgument(Param: String): String;
var
  ConfigFile: String;
begin
  ConfigFile := ExpandConstant('{param:HOUSECONFIGFILE|}');
  if ConfigFile = '' then
    Result := ''
  else begin
    if not FileExists(ConfigFile) then
      RaiseException('HOUSECONFIGFILE does not exist: ' + ConfigFile);
    Result := '--house-config-file "' + ConfigFile + '"';
  end;
end;

function OmpIntegrationArguments(Param: String): String;
begin
  Result :=
    '--omp-config "' + ExpandConstant('{%USERPROFILE}\.omp\agent\config.yml') + '" ' +
    '--client-config "' + ExpandConstant('{%USERPROFILE}\.omp\agent\athanor\client.json') + '" ' +
    '--operator-principal "' + ExpandConstant('{username}') + '"';
end;


procedure CurStepChanged(CurStep: TSetupStep);
var
  ResultCode: Integer;
  Parameters: String;
begin
  if CurStep = ssPostInstall then begin
    Parameters :=
      'install --staging "' + ExpandConstant('{tmp}\athanor-payload') + '" ' +
      '--manifest "' + ExpandConstant('{tmp}\athanor-payload\release-manifest.json') + '" ' +
      ExternalDatabaseArgument('') + ' ' +
      HouseConfigArgument('') + ' ' +
      OmpIntegrationArguments('');
    if not Exec(
      ExpandConstant('{app}\bin\athanor-manage.exe'),
      Parameters,
      ExpandConstant('{app}\bin'),
      SW_HIDE,
      ewWaitUntilTerminated,
      ResultCode
    ) then
      RaiseException('Failed to start the native Athanor manager.');
    if ResultCode <> 0 then
      RaiseException(Format('The native Athanor manager refused installation (exit %d).', [ResultCode]));
  end;
end;