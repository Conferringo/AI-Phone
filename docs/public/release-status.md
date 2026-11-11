# Releases / 发行

| Platform | Installer | Host |
| --- | --- | --- |
| macOS | [DMG 0.2.0](https://github.com/MrMilkDragon/AI-Phone/releases/download/v0.2.0/AImanity-0.2.0-macos-arm64.dmg) | Apple Silicon, macOS 14.4+ |
| Windows | [EXE 0.2.0](https://github.com/MrMilkDragon/AI-Phone/releases/download/v0.2.0/AImanity-0.2.0-windows-x64-setup.exe) | Windows 11 x64, hardware virtualization |

The installers include Android, Linux and local services. Open AImanity to configure your devices and model.

安装包内置 Android、Linux 和本地服务。打开 AImanity 后配置设备和模型。[Beta 安装与签名](#安装-beta)

[Release history](https://github.com/MrMilkDragon/AI-Phone/releases) · [Report an issue](https://github.com/MrMilkDragon/AI-Phone/issues/new/choose) · [备份恢复](backup-restore.zh-CN.md)

## Install the beta

Download the beta from GitHub Releases and install it manually. The in-app updater follows the stable channel. Before opening or re-signing a download, verify its SHA-256 against the `digest` for the exact asset filename and release tag in the official metadata. Stop if the hashes differ. [Releases](https://github.com/MrMilkDragon/AI-Phone/releases) · [Asset digests](https://api.github.com/repos/MrMilkDragon/AI-Phone/releases)

### macOS

The DMG and the app use ad hoc signatures. The app’s signature seals the bundle without a publisher identity. Developer ID identifies the publisher, and notarization is a separate Apple check. Gatekeeper can still block an ad hoc build. [Apple code signing](https://developer.apple.com/documentation/security/seccodesignatureflags/adhoc) · [Apple notarization](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)

In Terminal, enter the downloaded filename and its official digest when prompted.

```sh
cd "$HOME/Downloads"
printf 'DMG filename: '
IFS= read -r dmg
printf 'Official SHA-256: '
IFS= read -r digest
printf '%s  %s\n' "${digest#sha256:}" "$dmg" | shasum -a 256 -c -
```

After `OK`, mount the DMG, drag AImanity to Applications and open it. If Gatekeeper reports an unidentified developer or missing notarization, use **System Settings → Privacy & Security → Open Anyway** for this app if you trust the verified download. Keep Gatekeeper enabled. Stop if macOS reports malware or a damaged app. [Apple first-launch guidance](https://support.apple.com/en-us/102445)

For a local signing test, copy the app from the verified DMG to `~/Applications` and run the commands below. Finder displays AImanity. Its bundle is `AI Phone.app`.

```sh
app="$HOME/Applications/AI Phone.app"
codesign --force --sign - --timestamp=none "$app"
codesign --verify --deep --strict --verbose=2 "$app"
```

### Windows

The EXE carries a self-signed Authenticode test certificate. SmartScreen checks publisher and file reputation. Smart App Control can enforce stricter rules, and Defender Antivirus scans for threats. These checks also apply to locally signed copies. [SmartScreen](https://learn.microsoft.com/windows/apps/package-and-deploy/smartscreen-reputation) · [Smart App Control](https://learn.microsoft.com/windows/apps/develop/smart-app-control/overview) · [Defender Antivirus](https://support.microsoft.com/en-us/windows/security/threat-malware-protection/virus-and-threat-protection-in-the-windows-security-app)

Open Windows PowerShell. Enter the full installer path without surrounding quotes and paste the official digest.

```powershell
$installer = Read-Host 'Full path of the downloaded EXE'
$digest = (Read-Host 'Official SHA-256').Trim() -replace '^sha256:', ''
if ((Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash -ne $digest) {
    throw 'SHA-256 mismatch. Stop and download the release again.'
}
Get-AuthenticodeSignature -LiteralPath $installer |
    Select-Object Status, StatusMessage, SignerCertificate
```

After the hash matches, open the original EXE. If SmartScreen only reports an unrecognized app and offers **More info → Run anyway**, use that file-specific option only if you trust the download. Keep SmartScreen and Defender Antivirus enabled. Stop if Defender detects a threat or the device’s policy blocks the installer.

To test self-signing, sign a separate copy in the same PowerShell session. The test certificate is stored in your account’s personal certificate store. Windows may report its root as untrusted.

```powershell
$copy = "$installer.local-test.exe"
Copy-Item -LiteralPath $installer -Destination $copy
$cert = New-SelfSignedCertificate -Type CodeSigningCert `
    -Subject 'CN=AImanity Local Test' -CertStoreLocation Cert:\CurrentUser\My `
    -KeyExportPolicy NonExportable -NotAfter (Get-Date).AddDays(7)
Set-AuthenticodeSignature -LiteralPath $copy -Certificate $cert `
    -HashAlgorithm SHA256 -IncludeChain Signer | Out-Null
$signature = Get-AuthenticodeSignature -LiteralPath $copy
if ($signature.SignerCertificate.Thumbprint -ne $cert.Thumbprint) {
    throw 'The test signer does not match the generated certificate.'
}
$signature | Select-Object Status, StatusMessage, SignerCertificate
```

Remove the test certificate and key when finished using `Remove-Item -LiteralPath "Cert:\CurrentUser\My\$($cert.Thumbprint)" -DeleteKey`. [Create a test certificate](https://learn.microsoft.com/powershell/module/pki/new-selfsignedcertificate) · [Sign a file](https://learn.microsoft.com/powershell/module/microsoft.powershell.security/set-authenticodesignature)

## 安装 Beta

从 GitHub Releases 下载 Beta 后手动安装，应用内更新仍走稳定通道。打开或自签前，按版本标签和文件名找到官方元数据里的 `digest`，先核对 SHA-256。摘要不符时停止安装。 [下载](https://github.com/MrMilkDragon/AI-Phone/releases) · [官方摘要](https://api.github.com/repos/MrMilkDragon/AI-Phone/releases)

- **macOS**。DMG 和 App 使用临时 ad hoc 签名。它封存文件和资源，Developer ID 则用于识别发布者，公证由 Apple 单独执行。运行 [macOS 命令](#macos)核对摘要后，挂载 DMG，把 AImanity 拖进「应用程序」。若 Gatekeeper 只提示开发者身份或公证问题，且你信任已核对的文件，可在「系统设置 → 隐私与安全性」对这一个 App 选择「仍要打开」。若提示恶意软件或文件损坏，停止安装。测试自签时，把 App 拷到 `~/Applications` 后运行同一节的 `codesign` 命令。 [Apple 首次打开说明](https://support.apple.com/zh-cn/102445)
- **Windows**。EXE 使用 Authenticode 自签名测试证书。用 [PowerShell 命令](#windows)核对 SHA-256 后打开原安装包。如果 SmartScreen 只提示应用未知，且提供「更多信息 → 仍要运行」，仅在信任该文件时使用这一次放行。若 Defender 检测到威胁，或 Smart App Control、组织策略直接阻止，停止安装。测试自签时，用同一节的命令给副本签名，证书只存入当前用户的个人证书库。自签后仍可能出现 SmartScreen 提示或被策略阻止。保持系统防护开启。 [Microsoft 签名说明](https://learn.microsoft.com/windows/apps/package-and-deploy/smartscreen-reputation)
