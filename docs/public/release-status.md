# Release status

Updated 2026-11-08. Source code and technical documentation are public. **Current-source complete product installers are not published.**

## Distribution targets

| Host | Installer | Remaining work |
|---|---|---|
| macOS Apple Silicon | One DMG with local devices and services | Complete installer rebuild, notices/corresponding-source review; signing and notarization |
| Windows x64 | One EXE with native Android, WSL backend and offline WebView2 | Complete installer rebuild, notices/corresponding-source review; Windows boot and platform qualification |

Both clients provide an Android phone and a Linux desktop. Device/model configuration happens inside the client. Guest disks, accounts, conversations, files and credentials remain separate from versioned application resources. Installation never packages a live user's device or database.

Physical installation is waived for previews on the two distribution targets and remains untested. A waiver is not a passed check. Historical draft installers do not qualify the current source.

## Migration and acceptance

Rust owns the core Agent/device services and native host initialization, process supervision, device/media control and notification integration. Native Telegram, WeCom, Feishu, QQ and WhatsApp adapters and the LAN relay compile. The development installation now runs the Rust channel service and LAN relay; protobuf channel listing and LAN HTTP access pass. Real-account QR login and message delivery remain unverified. The channel JavaScript package and browser bootstrap are removed from the source candidate.

The candidate uses Rust for first-party runtime, installation and recovery; build/CI tooling uses TypeScript. Consolidated validation completed with 616 workspace checks, 99 isolated PostgreSQL checks, five existing process-recovery scenarios and 21 native-client checks passing. Strict Clippy and the production web build pass. Dependency notices have been regenerated. These results do not qualify complete installers, physical installations or every provider/device workflow.

Real phone/desktop observation and control have prior development-runtime evidence. They do not qualify new complete installers. Copilot's real microphone/provider quality, guest microphone routing and end-to-end voice latency remain unverified. Real incoming-message → desktop office work → source-chat delivery and all specialized application workflows still need complete acceptance. Model/channel presets are not certification of every service or account.

The publication gates in `config/release-policy.json` remain unsatisfied. Complete runtime contents, source/privacy review, third-party notices and applicable corresponding source are required. No incomplete installer is presented as a finished product.

[Distribution requirements](preview-release.md) · [Compatibility](compatibility.md) · [Android redistribution](engineering/android-distribution.md) · [Architecture](architecture.en.md) · [Backup and recovery](backup-restore.en.md)

## 中文

发行平台仅为 **Apple Silicon macOS（DMG）和 Windows x64（EXE）**。每个完整安装包内置 Android 手机、Linux 电脑及运行服务，用户在客户端配置设备和模型后使用。现有设备、账号、聊天和文件独立保存。

**当前源码的完整安装包尚未发行。** 候选中的运行、安装、恢复与渠道使用 Rust，构建和 CI 工具使用 TypeScript。集中验证、严格 Clippy、前端生产构建与依赖声明已完成；开发实例的渠道和局域网转发已切为 Rust，protobuf 列表与局域网 HTTP 已验。完整安装包重建及真实账号收发仍待完成，原设备、账号和聊天保持。

实机安装按两个发行平台豁免并记录为未测；历史草稿包不能替代当前版本验收。Copilot 实际语音质量、通知触发办公后回原会话交付及各类 App 特化仍未完成全部实测。发行门槛与第三方许可保持真实状态，不把计划或局部成功记为整体完成。
