; Terminaal's Windows setup, for Inno Setup 6 (preinstalled on GitHub's
; windows-latest runners). Built by CI (.github/workflows/release.yml, on
; a version tag):
;
;   ISCC /DAppVersion=0.3.1 /DSourceExe=C:\...\terminaal.exe /DOutputDir=C:\...\dist installer\terminaal.iss
;
; gives terminaal-x86_64-windows-setup.exe. What it does:
;
; - Installs into Program Files for all users by default, like other Windows
;   programs (UAC). The first page also offers "only for me"
;   (%LOCALAPPDATA%\Programs\Terminaal, no admin rights), for accounts that
;   can't elevate. An update or a repeated setup keeps the choice made the
;   first time.
; - Start menu shortcut carrying Terminaal's AppUserModelID, which the program
;   takes on when it runs from here (update::APP_USER_MODEL_ID): taskbar
;   grouping and toasts under Terminaal's name.
; - An entry under Settings -> Apps with the version and uninstall.
; - Optional: desktop shortcut, the drop-down window at sign-in.
; - Uninstall removes the program and its shortcuts; the settings only if the
;   user says so (never in a silent uninstall).
;
; Updates (src/update.rs) download the new setup and run it with
; /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /ALLUSERS (UAC) or /CURRENTUSER once
; Terminaal has ended; the setup closes a drop-down Terminaal of this install
; (Restart Manager), the update script starts it again afterwards.
;
; AppId ties this script to the program (update::SETUP_APP_ID, checked by a
; test): never change it, or updates and uninstall lose track of installs.

#ifndef AppVersion
  #error Pass the version: ISCC /DAppVersion=x.y.z
#endif
#ifndef SourceExe
  #define SourceExe "..\target\release\terminaal.exe"
#endif
#ifndef OutputDir
  #define OutputDir "..\dist"
#endif

[Setup]
AppId={{B9A7C3DB-EA83-4F63-A499-1A6BA7735663}
AppName=Terminaal
AppVersion={#AppVersion}
AppVerName=Terminaal {#AppVersion}
AppPublisher=Terminaal
AppPublisherURL=https://github.com/mergedeyes/terminaal
AppSupportURL=https://github.com/mergedeyes/terminaal/issues
AppUpdatesURL=https://github.com/mergedeyes/terminaal/releases
VersionInfoVersion={#AppVersion}
; {autopf}: Program Files for all users, %LOCALAPPDATA%\Programs per user.
DefaultDirName={autopf}\Terminaal
DisableProgramGroupPage=yes
; Program Files for all users (UAC); the first page offers "only for me".
PrivilegesRequired=admin
PrivilegesRequiredOverridesAllowed=dialog commandline
; The drop-down window at sign-in is per user even in an all-users install.
UsedUserAreasWarning=no
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Windows 10 1809: ConPTY.
MinVersion=10.0.17763
OutputDir={#OutputDir}
OutputBaseFilename=terminaal-x86_64-windows-setup
SetupIconFile=..\assets\terminaal.ico
UninstallDisplayIcon={app}\terminaal.exe
UninstallDisplayName=Terminaal
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
; A running Terminaal of this install (also a drop-down one) is closed so
; its exe can be replaced; nothing is restarted by the setup itself.
CloseApplications=force
RestartApplications=no

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "de"; MessagesFile: "compiler:Languages\German.isl"

[CustomMessages]
en.QuakeTask=Start the drop-down window when I sign in
de.QuakeTask=Dropdown-Fenster bei der Anmeldung starten
en.QuakeShortcut=Terminaal (drop-down window)
de.QuakeShortcut=Terminaal (Dropdown-Fenster)
en.RemoveSettings=Also delete your Terminaal settings?%n%nSettings, saved hosts, snippets, themes and the saved session are in:%n%1%n%2%n%nSSH keys in your .ssh folder stay either way.
de.RemoveSettings=Auch die Einstellungen von Terminaal löschen?%n%nEinstellungen, gespeicherte Hosts, eigene Befehle, Themes und die gespeicherte Sitzung liegen in:%n%1%n%2%n%nSSH-Schlüssel im Ordner .ssh bleiben so oder so erhalten.

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "quake"; Description: "{cm:QuakeTask}"; Flags: unchecked

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "terminaal.exe"; Flags: ignoreversion

; WorkingDir is written as is and expanded by Windows at each start: the
; first tab opens in the user's home, as Windows Terminal does, not in the
; program folder (an expanded {%USERPROFILE} would be the installing user's
; for everyone in an all-users install).
[Icons]
Name: "{autoprograms}\Terminaal"; Filename: "{app}\terminaal.exe"; WorkingDir: "%USERPROFILE%"; AppUserModelID: "Terminaal.Terminaal"
Name: "{autodesktop}\Terminaal"; Filename: "{app}\terminaal.exe"; WorkingDir: "%USERPROFILE%"; AppUserModelID: "Terminaal.Terminaal"; Tasks: desktopicon
Name: "{userstartup}\{cm:QuakeShortcut}"; Filename: "{app}\terminaal.exe"; Parameters: "--quake"; WorkingDir: "%USERPROFILE%"; Tasks: quake

[Run]
Filename: "{app}\terminaal.exe"; WorkingDir: "{%USERPROFILE}"; Description: "{cm:LaunchProgram,Terminaal}"; Flags: nowait postinstall skipifsilent runasoriginaluser

; Left behind by an in-place update of a bare exe copied here by hand.
[UninstallDelete]
Type: files; Name: "{app}\terminaal.exe.old"
Type: files; Name: "{app}\.terminaal-update-*"

[Code]
// Settings stay unless the user asks: never in a silent uninstall, and
// "No" is the default button.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Settings, State: String;
begin
  if (CurUninstallStep <> usPostUninstall) or UninstallSilent then
    Exit;
  Settings := ExpandConstant('{userappdata}\terminaal');
  State := ExpandConstant('{localappdata}\terminaal');
  if not DirExists(Settings) and not DirExists(State) then
    Exit;
  if MsgBox(FmtMessage(CustomMessage('RemoveSettings'), [Settings, State]), mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
  begin
    DelTree(Settings, True, True, True);
    DelTree(State, True, True, True);
  end;
end;
