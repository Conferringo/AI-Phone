[Setup]
AppId=dev.aiphone.client
AppName=AI Phone
AppVersion={#AppVersion}
VersionInfoVersion={#NumericVersion}
DefaultDirName={autopf}\AI Phone
DefaultGroupName=AI Phone
ArchitecturesAllowed=x64os
ArchitecturesInstallIn64BitMode=x64os
SetupArchitecture=x64
MinVersion=10.0.22000
PrivilegesRequired=admin
Compression=lzma2/ultra64
LZMANumBlockThreads=2
SolidCompression=yes
WizardStyle=modern
DisableProgramGroupPage=yes
UninstallDisplayIcon={app}\ai-phone-client.exe
SetupIconFile={#ProjectRoot}\apps\client\src-tauri\icons\icon.ico
LicenseFile={#ProjectRoot}\LICENSE

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "chinese"; MessagesFile: "compiler:Languages\ChineseSimplified.isl"
Name: "traditionalchinese"; MessagesFile: "compiler:Languages\ChineseTraditional.isl"
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"
Name: "korean"; MessagesFile: "compiler:Languages\Korean.isl"
Name: "german"; MessagesFile: "compiler:Languages\German.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "arabic"; MessagesFile: "compiler:Languages\Arabic.isl"

[Files]
Source: "{#AppSource}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\AI Phone"; Filename: "{app}\ai-phone-client.exe"; AppUserModelID: "dev.aiphone.client"

[Run]
Filename: "{app}\ai-phone-client.exe"; Description: "{cm:LaunchProgram,AI Phone}"; Flags: nowait postinstall skipifsilent; Check: not NeedRestart

[CustomMessages]
english.HostSetupFailed=Device runtime setup failed.
chinese.HostSetupFailed=设备运行环境安装失败。
traditionalchinese.HostSetupFailed=裝置執行環境安裝失敗。
japanese.HostSetupFailed=デバイス実行環境のセットアップに失敗しました。
korean.HostSetupFailed=기기 실행 환경을 설치하지 못했습니다.
german.HostSetupFailed=Die Geräteumgebung konnte nicht installiert werden.
french.HostSetupFailed=Échec de l’installation de l’environnement des appareils.
spanish.HostSetupFailed=No se pudo instalar el entorno de los dispositivos.
russian.HostSetupFailed=Не удалось установить среду устройств.
arabic.HostSetupFailed=تعذر تثبيت بيئة تشغيل الأجهزة.

[Code]
var
  HostRestart: Boolean;

procedure CurStepChanged(CurStep: TSetupStep);
var
  ResultCode: Integer;
  Output: TExecOutput;
  ErrorText: String;
  Index: Integer;
begin
  if (CurStep = ssPostInstall) and
    (FileExists(ExpandConstant('{app}\host\qemu\manifest.json')) or
     FileExists(ExpandConstant('{app}\host\windows\wsl.msi'))) then
  begin
    if not ExecAndCaptureOutput(ExpandConstant('{app}\ai-phone-client.exe'),
      '--prepare-windows-host', '', SW_HIDE, ewWaitUntilTerminated, ResultCode, Output) then
      RaiseException(SysErrorMessage(ResultCode));
    if (ResultCode <> 0) and (ResultCode <> 3010) then
    begin
      ErrorText := ExpandConstant('{cm:HostSetupFailed}') + ' (' + IntToStr(ResultCode) + ')';
      for Index := 0 to GetArrayLength(Output.StdOut) - 1 do
        ErrorText := ErrorText + #13#10 + Output.StdOut[Index];
      RaiseException(ErrorText);
    end;
    HostRestart := ResultCode = 3010;
  end;
end;

function NeedRestart(): Boolean;
begin
  Result := HostRestart;
end;
