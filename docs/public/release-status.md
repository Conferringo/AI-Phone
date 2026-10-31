# Release status / 发行状态

Updated 2026-10-31. Source code and technical documentation are available. Public product installers are subject to the complete distribution and redistribution gates.

## Platform qualification

| Target | Verified scope | Remaining qualification |
|---|---|---|
| macOS Apple Silicon | Complete nine-component App/DMG compiled; mounted-file verification; portable runtime execution; native migration and role checks; workbench startup | Fresh default-size dual-device installation, upgrades, signing/notarization and full acceptance |
| Windows x64 | Native client compilation; [sealed Android SDK/image component](engineering/windows-android-runtime.md), NTFS admission and packaged emulator/ADB execution | Complete independent host, backend, acceleration setup and first run |
| Linux x64 | Native client compilation, [offline host resources](container-host-runtime.md) and [isolated offline OCR](engineering/linux-offline-ocr.md) | Complete offline host, payload and first run |
| Linux ARM64 | Native client compilation | Matched Android runtime and complete independent host |
| Android ARM64 root/KVM | Native client compilation; privileged host target defined | Kernel/runtime integration, matched payload and both guests |

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

源码和技术文档已公开；公开安装包依照完整发行与第三方再分发门槛提供。

Apple Silicon完整App/DMG已编译，包含全部九类运行资源；只读映像文件逐项核对、系统隔离运行、包内原生程序迁移/数据库权限及工作台启动检查均已通过。默认规格双设备全新创建、升级、签名和完整产品验收仍待确认。Windows、Linux和root/KVM Android已具备客户端编译证据，完整独立宿主与配套运行资源仍需完成。

非macOS实机安装按本轮范围豁免；编译、完整内容、隐私与再分发检查继续保留。模型/渠道按各自协议及账号能力验收，测试替身不等于真实账号送达；Copilot的纠错和调度基础已接通，实际收音、语音识别/合成与设备麦克风路由尚未交付。所有发行门槛以版本配置及对应证据为准。
