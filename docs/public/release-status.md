# Install AImanity / 安装 AImanity

[0.0.2 Beryllium](https://github.com/MrMilkDragon/AImanity/releases/tag/v0.0.2) includes Android, the Linux desktop and local services.

| Platform | Requirements | Installer | Factory |
| --- | --- | --- | --- |
| macOS | Apple Silicon, macOS 15+ | [DMG](https://github.com/MrMilkDragon/AImanity/releases/download/v0.0.2/AImanity-0.0.2-macos-arm64.dmg) | [macOS](engineering/macos-full-runtime.md) |
| Windows | Windows 11 x64, hardware virtualization | [EXE](https://github.com/MrMilkDragon/AImanity/releases/download/v0.0.2/AImanity-0.0.2-windows-x64-setup.exe) | [Windows](windows-client-factory.md) |

Open the app, choose your device settings, then add a model in Configuration. Beryllium uses ad hoc signing on macOS and self-signed Authenticode on Windows.

- **macOS.** If macOS reports an unidentified developer, open **System Settings → Privacy & Security → Open Anyway** after trying to launch AImanity. [Apple first-launch help](https://support.apple.com/en-us/102445)
- **Windows.** If SmartScreen shows **Windows protected your PC**, choose **Run anyway** when offered for the EXE downloaded from the Releases link above. [Microsoft SmartScreen help](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation#certificate-options-and-their-smartscreen-implications)

安装包内置 Android、Linux 桌面和本机服务。打开客户端，配置两台设备，然后在配置中心添加模型。macOS 采用 ad hoc 签名，Windows 采用自签名 Authenticode。

- **macOS。** 如果系统提示开发者身份无法验证，尝试打开 AImanity 后，进入 **系统设置 → 隐私与安全性 → 仍要打开**。[Apple 首次打开指南](https://support.apple.com/zh-cn/102445)
- **Windows。** 如果 SmartScreen 提示 **Windows 已保护你的电脑**，从上方发行页下载 EXE 后，可在出现 **仍要运行** 时选择该项。[微软 SmartScreen 指南](https://learn.microsoft.com/zh-cn/windows/apps/package-and-deploy/smartscreen-reputation#certificate-options-and-their-smartscreen-implications)

[Model setup / 模型配置](model-and-channel-setup.md) · [Client development](native-client.en.md) · [客户端开发](native-client.zh-CN.md) · [Issues](https://github.com/MrMilkDragon/AImanity/issues/new/choose)
