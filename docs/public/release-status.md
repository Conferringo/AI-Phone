# Release status / 发行状态

Updated 2026-11-01. Source code and technical documentation are available. **Complete product installers are not published.** Internal package builds and component checks do not replace first-run acceptance or redistribution approval. This page is the current platform-status reference; installation guides describe their respective setup paths.

## Platform qualification

| Target | Build and component evidence | Remaining qualification |
|---|---|---|
| macOS Apple Silicon | Complete nine-component App/DMG compiled internally; mounted-file verification; portable runtime execution; native migration and role checks; workbench startup | Complete first-time default-size dual-device initialization has not passed; upgrades, signing/notarization and full acceptance remain open |
| Windows x64 | Native client compilation; [sealed Android SDK/image component](engineering/windows-android-runtime.md), NTFS admission, packaged emulator/ADB and [portable installer runtime](engineering/installer-tools.md) execution; [WSL2 backend component implementation](../../scripts/wsl_backend_factory.py) | Components are not yet connected into a complete Windows host: native phone/WSL bridge, distro lifecycle, acceleration setup and first run remain incomplete |
| Linux x64 | [Complete nine-component Debian package](linux-client-factory.md) compiled and inspected internally; packaged web/native/OCR execution | Complete first-time dual-device initialization has not passed; physical installation remains untested under the maintainer waiver |
| Linux ARM64 | Native client compilation | Matched Android runtime and complete independent host |
| Android ARM64 root/KVM | Native client compilation; privileged host target defined | Kernel/runtime integration, matched payload and complete dual-guest initialization remain incomplete |

The required experience is installation followed by device/model configuration and workbench use. System images, emulator, services and execution runtimes belong in the distribution. [The Apple Silicon package structure](engineering/macos-full-runtime.md) implements this complete resource profile; [runtime bundles](runtime-bundles.md) documents the format and build interface.

`config/release-policy.json` owns promotion gates. Binary publication remains disabled and the all-platform `self_contained_device_bundle` gate remains false. Non-macOS physical installation is waived for this release scope; compilation, complete contents, privacy and redistribution checks remain required.

## Runtime and product acceptance

- Android has real ARM64 boot/root/display/input and persistence evidence. Linux has offline import, GUI/input, notification and audio-capture evidence. Fresh default-size creation and the complete installer lifecycle require separate acceptance.
- The complete macOS disk image contains the web, native services, phone applications, container host, JavaScript services, guest applications, Android runtime, Linux/data images and portable Python/Node. Mounted resources passed WAN/source/SDK-denied execution checks. The packaged assistant applied thirty migrations through schema version 32; its runtime role cannot perform DDL.
- [Thirteen model presets and connection setup](model-and-channel-setup.md) have isolated protocol/transport checks. Recorded provider probes and WhatsApp QR/bootstrap evidence do not certify every provider account, channel delivery or current quota. User API credentials are not distribution inputs.
- [Copilot](copilot-voice.md) has deterministic endpointing, durable correction receipts and voice-priority steering. Microphone capture, speech providers, streaming audio, spoken interruption and guest microphone routing are not available yet.
- [Private installation state](engineering/private-install-state.md) separates checkpoints, file locks and atomic writes. Signed updates, complete long-running recovery and the full UI/language/media matrix remain open.

Architecture findings and remaining complexity are recorded in the [SOLID audit](solid-audit.md). Platform details, distribution requirements and recovery procedures are in [compatibility](compatibility.md), [bundle requirements](preview-release.md) and [database recovery](backup-restore.en.md). Component tests, compilation and installation acceptance are recorded independently.

## 中文

源码和技术文档已公开；**完整产品安装包尚未发布**。本页统一记录各平台状态。内部编译、包内检查不等于全新安装通过，也不代替第三方再分发许可。

Apple Silicon 完整 App/DMG 已在内部编译，包含全部九类运行资源；映像文件核对、隔离运行、包内原生程序迁移/数据库权限及工作台启动检查已通过。Linux x64 完整九类资源 DEB 也已内部编译、解包核验并执行包内网页/原生/OCR 检查。两者的首次完整双设备初始化尚未通过，升级及完整产品验收仍待完成，macOS 还需签名与公证。

Windows 已有原生 SDK/系统底图、Python/Node 执行证据及 WSL2 后端组件实现，但尚未接通完整宿主，原生手机与 WSL 桥接、发行版生命周期及首次运行仍不完整。Linux ARM64 缺匹配的完整宿主资源；root/KVM Android 的内核/运行时、配套镜像与完整双客体初始化尚未完成。

非macOS实机安装按当前验收范围豁免；编译、完整内容、隐私与再分发检查继续保留。模型/渠道按各自协议及账号能力验收，测试替身不等于真实账号送达；Copilot的纠错和调度基础已接通，实际收音、语音识别/合成与设备麦克风路由尚未交付。所有发行门槛以版本配置及对应证据为准。
