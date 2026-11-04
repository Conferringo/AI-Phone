# Release status

Updated 2026-11-04. Source code and technical documentation are available. **Complete product installers are not published.** This page is the current reference for platform availability and known product limits.

## Platform qualification

| Target | Available implementation | Remaining delivery requirements |
| --- | --- | --- |
| macOS Apple Silicon | Native client and inspected complete App/DMG factory output | Fresh default-size dual-device installation, upgrades, signing/notarization and complete product acceptance |
| Windows x64 | Native client and Android/tools components; offline WSL2 backend with Rust registration and initialization | Native Android/WSL bridge, acceleration setup and complete first run |
| Linux x64 | Native client and inspected complete DEB/AppImage factory outputs | Single-file distribution, installed-client lifecycle and applicable source/licence review; physical installation remains untested |
| Linux ARM64 | Native client build | Matched Android/runtime resources and a complete independent host |
| Android ARM64 root/KVM | Native client and privileged-host target | Kernel/runtime integration, matched resources and complete dual-guest initialization |

The required experience is installation followed by device/model configuration and workbench use. Android and Linux baselines, emulators, services and execution runtimes belong in the distribution. The [Apple Silicon resource profile](engineering/macos-full-runtime.md), [Linux factory](linux-client-factory.md) and [runtime format](runtime-bundles.md) describe those package boundaries.

The complete [Mac factory](https://github.com/Conferringo/AI-Phone/actions/runs/41758075411) and [Linux factory](https://github.com/Conferringo/AI-Phone/actions/runs/41758900254) succeeded from `e9ecab3`. Their DMG, DEB and AppImage include nine runtime resource groups and remain unpublished maintainer drafts. The installers still exceed the project's single-asset transport limit; internal volumes do not meet the single-download requirement. Successful factory output does not certify installation, source/licence closure or independent Windows/Android hosting.

## Capability limits

- Real Android/Linux execution, input and persisted state exist in the development runtime. Component startup does not establish a complete first-time client installation.
- [Models and connections](model-and-channel-setup.md) include provider presets and protocol adapters. Support remains dependent on the configured model's capabilities and the application account; a preset is not certification of every provider or delivery path.
- [Copilot](copilot-voice.md) connects browser PCM capture, Rust/protobuf REST and persistent live-transcription adapters, durable recognized-text submission and interruptible playback. Real microphone/provider quality, echo, end-to-end latency and guest microphone routing remain unverified. Live input does not imply streaming speech output or guest calls.
- Authenticated update discovery, complete activation/rollback and the full platform/language/media matrix remain in development. User data is kept separately from versioned program resources.

## Distribution requirements

`config/release-policy.json` owns publication gates. Binary publication remains disabled and `self_contained_device_bundle` remains false. Packages must satisfy full contents, source/privacy checks, third-party notices and applicable corresponding-source requirements before publication.

Non-macOS physical installation is waived for the current release scope and remains unverified. Compilation and component checks do not replace that result or qualify incomplete packages.

[Compatibility](compatibility.md) · [Distribution requirements](preview-release.md) · [Architecture](architecture.en.md) · [Contributing](../../CONTRIBUTING.md) · [Backup and recovery](backup-restore.en.md)

## 中文

源码与技术文档已公开，**完整产品安装包尚未发布**。macOS Apple Silicon 的完整 DMG、Linux x64 的完整 DEB/AppImage 工厂已成功，包含九类运行资源，仍保留为未公开草稿。安装器仍超过本项目单附件传输上限，内部运输分卷不等于面向用户的单文件下载。Windows 已接通 Rust 专属 WSL 注册及后端初始化，原生 Android 控制/媒体桥与完整首次就绪仍未完成。Linux ARM64 与 root/KVM Android 仍缺匹配资源及完整双客体初始化。

Copilot 已接通浏览器 PCM 收音、Rust/protobuf 语音适配、连续文字提交与可打断播放；真实麦克风及服务商质量、回声、端到端速度和客体麦克风路由仍未验收。模型与连接应用按协议、模型能力及账号分别支持；配置预设不代表全部路径均已验收。

发行须满足完整组件、隐私、许可、对应源码及安装生命周期要求。非 macOS 实机安装按当前范围豁免并记为未测，不能据此把缺少组件或尚未接通的宿主标为完成。版本代码与设备、账号、聊天及用户文件保持分离。
