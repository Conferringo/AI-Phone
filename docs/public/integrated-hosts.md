# Self-contained host runtime

The installed client creates its phone, computer and model configuration without a manual server deployment. The target distributions are one Apple Silicon macOS DMG and one Windows x64 EXE. See [release status](release-status.md) for actual installer availability and qualification.

## Host architecture

| Host | Phone | Computer and backend |
|---|---|---|
| macOS Apple Silicon | Native ARM64 Android Emulator using HVF | App-owned Lima/VZ container host |
| Windows x64 | Native x64 Android Emulator using WHPX | App-owned WSL2 distribution |

The default guests are Android 15 and Debian 13/Openbox. CPU, memory and initial phone storage are configurable. Existing device storage is preserved when creation defaults change. The Linux desktop shares its container host's kernel.

## Installation lifecycle

1. Choose language and device resources. A fresh complete installation requires 12 GiB RAM and 35 GiB free storage; 16 GiB RAM is recommended.
2. The client stages the bundled systems, emulator, container engine, applications and compiled services. Users do not prepare a development environment or download system images separately.
3. Versioned code lives in `host/releases/<payload hash>`; devices, database, files and configuration live in `host/user`.
4. Rust initializes the container host, images, database and services, boots the native phone, provisions input/notifications and checks actual device frames before reporting ready.
5. Resume reuses existing profiles, credentials, devices and running services. Cancellation retains completed work and preserves unknown device outcomes for observation.

The Rust host runtime owns source staging, device creation, pairing and process supervision. SQLx owns PostgreSQL role preparation and migrations. The assistant also contains messaging-channel adapters; the phone runner owns media/control. Next.js/TypeScript supplies the interface, with Bun for frontend execution and build tooling.

The Windows client initializes WSL through bounded stdin configuration, then starts the native Android worker and paired backend. Backend acknowledgement alone does not establish phone or full installation readiness. Device setup completes through its protobuf receipt.

## Qualification

The current migration candidate has not completed consolidated validation or rebuilt complete installers. Windows WSL/WHPX/firewall behavior, full first-run integration, upgrades and foreground audio remain separate acceptance items. Physical installation is waived for preview packages and must be disclosed as untested. Compilation and third-party redistribution review remain required.

## 中文

发行目标为 Apple Silicon macOS DMG 和 Windows x64 EXE，随包包含系统、模拟器、容器宿主、应用和服务。用户在客户端选择语言、设备资源并配置模型后使用，无需另外部署服务。

Rust 负责安装、恢复、进程、设备、媒体和渠道，SQLx 负责数据库角色和迁移。程序版本与用户数据分开保存，恢复已有安装不重建设备或覆盖账号聊天。WSL 启动成功不等于手机或双设备初始化完成，实际就绪由 protobuf 回执确认。

当前迁移候选仍待集中验证和完整安装包重建；物理安装豁免不记作通过。完整首启、升级、前台声音及第三方再分发资格分别记录。
