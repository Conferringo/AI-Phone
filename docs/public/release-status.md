# Release status

Updated 2026-11-08. Source code and technical documentation are public. **Current-source complete product installers are not published.**

## Distribution targets

| Host | Installer | Remaining work |
|---|---|---|
| macOS Apple Silicon | One DMG with local devices and services | Complete migration, consolidated validation, rebuild, notices/corresponding-source review; signing and notarization |
| Windows x64 | One EXE with native Android, WSL backend and offline WebView2 | Complete migration, consolidated validation, rebuild, notices/corresponding-source review; Windows boot and platform qualification |

Both clients provide an Android phone and a Linux desktop. Device/model configuration happens inside the client. Guest disks, accounts, conversations, files and credentials remain separate from versioned application resources. Installation never packages a live user's device or database.

Physical installation is waived for previews on the two distribution targets and remains untested. A waiver is not a passed check. Historical draft installers do not qualify the current source.

## Migration and acceptance

Rust owns the core Agent/device services and native host initialization, process supervision, device/media control and notification integration. The current migration candidate adds native Telegram, WeCom, Feishu, QQ and WhatsApp adapters and the LAN relay; their current-source compilation and delivery validation are pending. The old channel JavaScript package and browser bootstrap are removed from that candidate. Existing live installations have not been silently replaced.

The candidate uses Rust for first-party runtime, installation and recovery; build/CI tooling uses TypeScript. Consolidated validation, dependency inventories and complete installer rebuilds are still pending. The live development installation has not been replaced.

Real phone/desktop observation and control have prior development-runtime evidence. They do not qualify the latest uncompiled candidate. Copilot's real microphone/provider quality, guest microphone routing and end-to-end voice latency remain unverified. Real incoming-message → desktop office work → source-chat delivery and all specialized application workflows still need complete acceptance. Model/channel presets are not certification of every service or account.

The publication gates in `config/release-policy.json` remain unsatisfied. Complete runtime contents, source/privacy review, third-party notices and applicable corresponding source are required. No incomplete installer is presented as a finished product.

[Distribution requirements](preview-release.md) · [Compatibility](compatibility.md) · [Android redistribution](engineering/android-distribution.md) · [Architecture](architecture.en.md) · [Backup and recovery](backup-restore.en.md)

## 中文

发行平台仅为 **Apple Silicon macOS（DMG）和 Windows x64（EXE）**。每个完整安装包内置 Android 手机、Linux 电脑及运行服务，用户在客户端配置设备和模型后使用。现有设备、账号、聊天和文件独立保存。

**当前源码的完整安装包尚未发行。** 候选中的运行、安装、恢复与渠道使用 Rust，构建和 CI 工具使用 TypeScript。集中验证、依赖清单及完整安装包重建仍待完成，现有运行实例没有被替换。

实机安装按两个发行平台豁免并记录为未测；历史草稿包不能替代当前版本验收。Copilot 实际语音质量、通知触发办公后回原会话交付及各类 App 特化仍未完成全部实测。发行门槛与第三方许可保持真实状态，不把计划或局部成功记为整体完成。
