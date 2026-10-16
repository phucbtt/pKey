; pKey for Windows: per-user installer (no administrator rights needed).
; Build with scripts\windows\build-installer.ps1, which compiles everything first.
;
; Two ways to type Vietnamese, installed side by side:
;   * pKey app: a tray program that works in every application at once.
;   * pKey keyboard (optional): a Windows text service. Use one of the two, not both.
; The text service is registered for the current user only (HKCU) with `regsvr32 /n /i:user`.
; A machine-wide, signed installer is a separate deliverable.

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#ifndef BuildDir
  #define BuildDir "..\..\target\installer"
#endif

[Setup]
AppId={{6F1D2B7C-3A54-4E0B-9C8D-1B7E5A42D9F3}
AppName=pKey
AppVersion={#AppVersion}
AppPublisher=pKey
DefaultDirName={localappdata}\pKey
DisableDirPage=yes
; The folder is now called pKey; an install from the VietIME days is cleaned up above.
UsePreviousAppDir=no
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
; Runs on 32-bit and 64-bit Windows 7 SP1 and newer (MinVersion defaults to 6.1sp1). On 64-bit
; Windows Setup runs in 64-bit mode so {sys} is the real System32 (the 64-bit regsvr32).
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\..\target\installer
OutputBaseFilename=pKey-Setup-{#AppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayName=pKey (Vietnamese input method)
CloseApplications=no
RestartApplications=no
AlwaysRestart=no

[Types]
Name: "full"; Description: "pKey app (recommended)"
Name: "custom"; Description: "Custom"; Flags: iscustom

[Components]
Name: "app"; Description: "pKey app (tray program, works in every application)"; Types: full custom; Flags: fixed
Name: "tsf"; Description: "Windows keyboard 'pKey' (alternative; do not use together with the app)"; Types: custom

[Tasks]
Name: "autostart"; Description: "Start pKey when I sign in to Windows"
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
; The app is one 32-bit build for every Windows: it runs on x86, x64 and ARM64.
Source: "{#BuildDir}\x86\pkey.exe"; DestDir: "{app}\bin"; Components: app; Flags: ignoreversion restartreplace
Source: "{#BuildDir}\x86\pkey-cli.exe"; DestDir: "{app}\bin"; Components: app; Flags: ignoreversion
Source: "{#BuildDir}\x64\pkey_tsf.dll"; DestDir: "{app}\bin\x64"; Components: tsf; Check: Is64BitInstallMode; Flags: ignoreversion restartreplace uninsrestartdelete
Source: "{#BuildDir}\x86\pkey_tsf.dll"; DestDir: "{app}\bin\x86"; Components: tsf; Flags: ignoreversion restartreplace uninsrestartdelete
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[InstallDelete]
; Left over from when the app was called VietIME (and installed in its own folder).
Type: filesandordirs; Name: "{localappdata}\VietIME\bin"
Type: files; Name: "{app}\bin\vietime-app.exe"
Type: files; Name: "{app}\bin\vietime.exe"
Type: files; Name: "{autoprograms}\VietIME.lnk"
Type: files; Name: "{autodesktop}\VietIME.lnk"

[Icons]
Name: "{autoprograms}\pKey"; Filename: "{app}\bin\pkey.exe"; Components: app
Name: "{autodesktop}\pKey"; Filename: "{app}\bin\pkey.exe"; Components: app; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "pKey"; ValueData: """{app}\bin\pkey.exe"""; Components: app; Tasks: autostart; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "VietIME"; Flags: deletevalue

[Run]
Filename: "{sys}\regsvr32.exe"; Parameters: "/s /n /i:user ""{app}\bin\x64\pkey_tsf.dll"""; Components: tsf; Check: Is64BitInstallMode; Flags: runhidden; StatusMsg: "Registering the 64-bit keyboard..."
Filename: "{syswow64}\regsvr32.exe"; Parameters: "/s /n /i:user ""{app}\bin\x86\pkey_tsf.dll"""; Components: tsf; Flags: runhidden; StatusMsg: "Registering the 32-bit keyboard..."
Filename: "{app}\bin\pkey.exe"; Description: "Start pKey now"; Components: app; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/f /im pkey.exe"; Flags: runhidden; RunOnceId: "StopApp"
Filename: "{sys}\regsvr32.exe"; Parameters: "/s /u /n /i:user ""{app}\bin\x64\pkey_tsf.dll"""; Check: Is64BitInstallMode; Flags: runhidden; RunOnceId: "UnregX64"
Filename: "{syswow64}\regsvr32.exe"; Parameters: "/s /u /n /i:user ""{app}\bin\x86\pkey_tsf.dll"""; Flags: runhidden; RunOnceId: "UnregX86"

[Code]
// An upgrade must unregister the old keyboard DLLs first, or the new registration would point at
// files that are about to be replaced.
procedure UnregisterExisting();
var
  ResultCode: Integer;
  Old64, Old32: String;
begin
  Old64 := ExpandConstant('{app}\bin\x64\pkey_tsf.dll');
  Old32 := ExpandConstant('{app}\bin\x86\pkey_tsf.dll');
  if FileExists(Old64) then
    Exec(ExpandConstant('{sys}\regsvr32.exe'), '/s /u /n /i:user "' + Old64 + '"', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  if FileExists(Old32) then
    Exec(ExpandConstant('{syswow64}\regsvr32.exe'), '/s /u /n /i:user "' + Old32 + '"', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ResultCode: Integer;
begin
  // A running app keeps its exe locked (the old name too, from before the rename).
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/f /im vietime-app.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/f /im pkey.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  UnregisterExisting();
  Result := '';
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if (CurStep = ssPostInstall) and WizardIsComponentSelected('tsf') then
    MsgBox('The Windows keyboard is installed. To use it, add it: Settings > Time & language > ' +
      'Language & region > Vietnamese > Language options > Add a keyboard > pKey, then pick it ' +
      'with Win+Space. Do not run the pKey app at the same time.' + #13#10#13#10 +
      'Applications that are already running need to be restarted first.',
      mbInformation, MB_OK);
end;
