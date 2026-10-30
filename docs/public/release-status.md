# Release status / 发行状态

Updated 2026-10-30. **Development; no complete product distribution is available.**

Incomplete preview downloads and their release tags have been withdrawn. Source and active development reviews remain available. No replacement is published until the complete-device-bundle gate passes.

The required user experience is: install the distribution, configure phone/computer/model in the client, then use the workbench. The package must contain matched system images, emulator and prebuilt services. Requiring users to acquire SDK/images, compile services or manually deploy a backend does not meet this standard.

| Target | Development evidence | Remaining distribution gate |
|---|---|---|
| macOS ARM64 | Existing Android/Linux runtime, native setup and recovery tests | Complete redistributable payload, offline materialization, first-run acceptance |
| Windows x64 | Native client compilation | Native phone + backend supervision, complete payload and first run |
| Linux x64 | Managed worker under review | Full runtime acceptance and complete payload |
| Linux ARM64 | Client compilation | Reviewed architecture-matched Android runtime and payload |
| Android ARM64 root/KVM | Host scope selected | Kernel/permission checks, matching privileged runtime and both guests |

`config/release-policy.json` is the promotion gate owner. Binary publication is disabled and `self_contained_device_bundle` is false. A prior waiver of physical installer tests does not waive complete package contents or redistribution requirements. Source CI and client builds continue as development checks; they are not downloadable product releases.

[Thirteen model presets and local WhatsApp QR](model-and-channel-setup.md) are implemented with isolated protocol/transport fixtures and real QR-bootstrap evidence. Paid-provider and WhatsApp account-level acceptance remain pending. Channel certification, long-running recovery, signed updates and full UI/language/media acceptance remain open. See [SOLID audit](solid-audit.md), [compatibility](compatibility.md), [bundle requirements](preview-release.md) and [database recovery](backup-restore.en.md).

---

# 中文状态

当前没有完整产品安装包。此前不完整的预览版与标签已撤回，源码与当前研发审阅保留。完整包应包含系统镜像、模拟器与预构建服务，初始化时自动部署，再由用户配置设备资源与模型；不能把源码安装或连接客户端当成最终发行。

二进制发布开关已关闭，新增“完整设备包”门槛且保持未通过。Android 以 root＋KVM 为宿主范围。四平台运行组件、镜像/模拟器再分发依据、完整包校验与初始化验收都必须明确，不用编译通过、页面入口或文档代替运行事实。
