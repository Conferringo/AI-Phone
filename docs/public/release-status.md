# Release status

Updated 2026-11-01. Source code and technical documentation are available. **Complete product installers are not published.** This page is the current reference for platform availability and known product limits.

## Platform qualification

| Target | Available implementation | Remaining delivery requirements |
| --- | --- | --- |
| macOS Apple Silicon | Native client and complete runtime resource profile; internal App/DMG builds | Fresh default-size dual-device installation, upgrades, signing/notarization and complete product acceptance |
| Windows x64 | Native client, Android runtime and portable tools; offline WSL2 backend component | Client-owned distro lifecycle, native Android/WSL bridge, acceleration setup and complete first run |
| Linux x64 | Native client and complete Debian package factory | Complete installed-client dual-device lifecycle; physical installation remains untested |
| Linux ARM64 | Native client build | Matched Android/runtime resources and a complete independent host |
| Android ARM64 root/KVM | Native client and privileged-host target | Kernel/runtime integration, matched resources and complete dual-guest initialization |

The required experience is installation followed by device/model configuration and workbench use. Android and Linux baselines, emulators, services and execution runtimes belong in the distribution. The [Apple Silicon resource profile](engineering/macos-full-runtime.md), [Linux factory](linux-client-factory.md) and [runtime format](runtime-bundles.md) describe those package boundaries.

## Capability limits

- Real Android/Linux execution, input and persisted state exist in the development runtime. Component startup does not establish a complete first-time client installation.
- [Models and connections](model-and-channel-setup.md) include provider presets and protocol adapters. Support remains dependent on the configured model's capabilities and the application account; a preset is not certification of every provider or delivery path.
- [Copilot](copilot-voice.md) has endpointing, correction receipts and voice-priority input contracts. Actual microphone capture, speech providers, interruptible playback and guest microphone routing are not available yet.
- Authenticated update discovery, complete activation/rollback and the full platform/language/media matrix remain in development. User data is kept separately from versioned program resources.

## Distribution requirements

`config/release-policy.json` owns publication gates. Binary publication remains disabled and `self_contained_device_bundle` remains false. Packages must satisfy full contents, source/privacy checks, third-party notices and applicable corresponding-source requirements before publication.

Non-macOS physical installation is waived for the current release scope and remains unverified. Compilation and component checks do not replace that result or qualify incomplete packages.

[Compatibility](compatibility.md) · [Distribution requirements](preview-release.md) · [Architecture](architecture.en.md) · [Contributing](../../CONTRIBUTING.md) · [Backup and recovery](backup-restore.en.md)

## 中文

源码与技术文档已公开，**完整产品安装包尚未发布**。macOS Apple Silicon 和 Linux x64 已有完整资源结构及内部构建；Windows 已有原生 Android/工具和离线 WSL2 后端组件，完整宿主接线仍未完成。Linux ARM64 与 root/KVM Android 仍缺匹配资源及完整双客体初始化。

实际收音、语音服务、可打断播放和客体麦克风路由尚未交付。模型与连接应用按协议、模型能力及账号分别支持；配置预设不代表全部路径均已验收。

发行须满足完整组件、隐私、许可、对应源码及安装生命周期要求。非 macOS 实机安装按当前范围豁免并记为未测，不能据此把缺少组件或尚未接通的宿主标为完成。版本代码与设备、账号、聊天及用户文件保持分离。
