# Release status / 发行状态

Updated 2026-10-31. **Development; no complete product distribution is available.**

Source, technical documentation and contributor build instructions are available. Product installers require the complete-device-bundle gate to pass.

The required user experience is: install the distribution, configure phone/computer/model in the client, then use the workbench. The package must contain matched system images, emulator and prebuilt services. Requiring users to acquire SDK/images, compile services or manually deploy a backend does not meet this standard.

| Target | Development evidence | Remaining distribution gate |
|---|---|---|
| macOS ARM64 | Sealed Android runtime, network-denied extraction, isolated root/display/restart checks; native setup and recovery tests | Complete redistributable payload, 16 GiB device creation and full first-run acceptance |
| Windows x64 | Native client compilation | Native phone + backend supervision, complete payload and first run |
| Linux x64 | Managed worker under review | Full runtime acceptance and complete payload |
| Linux ARM64 | Client compilation | Reviewed architecture-matched Android runtime and payload |
| Android ARM64 root/KVM | Host scope selected | Kernel/permission checks, matching privileged runtime and both guests |

`config/release-policy.json` is the promotion gate owner. Binary publication is disabled and `self_contained_device_bundle` is false. A prior waiver of physical installer tests does not waive complete package contents or redistribution requirements. Source CI and client builds continue as development checks; they are not downloadable product releases.

[Prebuilt runtime components](runtime-bundles.md) contain Next standalone, pinned Bun and host-matched Rust assistant/runner binaries with OCR. Relocated web startup, native version/OCR execution and 29 embedded database migrations have passed locally on macOS; the runtime database role cannot perform DDL. Matching macOS/Linux native-component CI has passed, including real OCR and staged execution. The optional [Android development component](engineering/android-runtime.md) now has macOS boot evidence. An optional [Linux desktop and service image component](engineering/linux-image-runtime.md) has real ARM64 offline-import, GUI, input, notification and audio-capture evidence. Its container host, media/channel runtimes, per-platform integration and redistribution clearance remain distribution work.

[Thirteen model presets and local WhatsApp QR](model-and-channel-setup.md) are implemented with isolated protocol/transport fixtures and real QR-bootstrap evidence. The saved model connection passed authorized text, image and complete tool-roundtrip probes without sending chats or device state; other providers and WhatsApp account-level acceptance remain pending. Channel certification, long-running recovery, signed updates and full UI/language/media acceptance remain open. See [SOLID audit](solid-audit.md), [compatibility](compatibility.md), [bundle requirements](preview-release.md) and [database recovery](backup-restore.en.md).

The optional [portable installer tool component](engineering/installer-tools.md) now executes Python/Node directly instead of downloading those interpreters. Native staging, source extraction and runtime-path restoration passed in a network-denied macOS sandbox. This component is not yet a rebuilt complete installer; phone applications, the container host and media/channel runtimes remain separate distribution work.

---

# 中文状态

当前没有完整产品安装包。源码、技术文档与贡献者构建说明已提供。完整包应包含系统镜像、模拟器与预构建服务，初始化时自动部署，再由用户配置设备资源与模型；不能把源码安装或连接客户端当成最终发行。

二进制发布要求“完整设备包”门槛通过；当前仍有待完成的组件。Android 以 root＋KVM 为宿主范围。四平台运行组件、镜像/模拟器再分发依据、完整包校验与初始化验收都必须明确，不用编译通过、页面入口或文档代替运行事实。

随包 Python/Node 已完成 macOS 系统禁网环境下的原生复制、源码展开与运行路径恢复，尚未重建为完整产品安装包；手机应用、容器宿主及媒体/渠道运行组件继续补齐。
