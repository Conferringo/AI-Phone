# Windows backend runtime

The factory assembles an offline Linux root filesystem for an application-owned WSL2 distribution. End users do not compile the backend or prepare container images. Native Windows Android/WHPX and WSL integration require separate qualification.

## Contents

| Resource | Contract |
|---|---|
| Ubuntu base | Immutable base digest and retained distribution notices |
| Rust services | Assistant with channels, phone runner/media, OCR and native initializer |
| Web | Next.js standalone resources and matched Bun executable |
| Docker and Compose | Pinned Alpine package closure in an isolated musl prefix |
| Container images | Sealed workspace, desktop, PostgreSQL and Redis images |
| Phone bridge | Linux x64 ADB, upstream notices and the emulator media schema |
| User state | Created under `/var/lib/aiphone`; excluded from the distributed rootfs |

The WSL factory is TypeScript build tooling. It extracts the source-bound Rust bootstrap and assembles the clean image directly. It does not export an existing user container. Phone applications and guest Clash APKs are optional paired component groups; complete Windows assembly requires both groups.

The outer capsule contains `rootfs.tar.gz` and `WSL-BACKEND-MANIFEST.json`. Its manifest binds the compressed body, uncompressed inventory, required native entries, source/image identities and base license files. Rootfs inspection streams decompression and hashing together. Prepared native resources contain `rootfs.tar.gz` and `manifest.json`.

## Native initialization

`aiphone-bootstrap --initialize-wsl-backend` reads configuration from stdin: locale, phone CPU/RAM/storage and desktop CPU/RAM. The configuration remains bounded to 8 KiB and uses the shared host-profile policy. Resume preserves profiles, passwords, tokens, devices and chats.

The installation is `/var/lib/aiphone/installation`; the owned engine socket is `unix:///run/aiphone/docker.sock`. Source staging, engine and image activation, database role preparation, migrations and service startup are Rust operations. Initialization imports bundled images without pulling or building them. It does not import a host Docker context or expose Windows drive mounts.

`backend_started: true` confirms the backend only. `phone_ready`, `complete_device_bundle` and `windows_boot_verified` remain false in that acknowledgement. The Windows native client subsequently boots or resumes Android and establishes the paired ADB/media connection. Device provisioning installs input/notification resources, creates or recovers the desktop, observes frames and emits `NativeWslDevicesReady` over protobuf. Only that receipt completes device setup.

Interrupted actions retain their installation/provisioning records. Recovery observes unknown outcomes rather than recreating a device or blindly repeating an action. Registered distributions retain their installed bootstrap; in-place backend upgrades are a separate lifecycle.

## Maintainer factory

```sh
./scripts/bootstrap.sh
./scripts/bun scripts/wsl-backend-qualification.ts --with-device-apps
```

Use an explicitly disposable GitHub-hosted Linux x64 workspace with `AI_PHONE_DISPOSABLE_QUALIFICATION=1`. The optional phone applications require Android 35 build tools and Java 17. The factory builds the same sealed source and images used by the backend, then imports the resulting rootfs into an owned, isolated container.

The existing Rust `release-tools wsl-qualify` worker runs as PID 1. It invokes the actual native initializer and verifies services, database roles, Redis, real desktop frames/media, work-file persistence and a database marker. Two phases reuse the same owned volume and verify restart persistence. The outer TypeScript driver records the worker receipts and removes only its labelled container, image and volume. Private logs, root filesystems and device/account data are not public evidence.

These checks establish Linux backend behavior; they do not establish native Windows WSL/WHPX/firewall or physical installation behavior. The current migration candidate still awaits consolidated validation and rebuilt complete installers. See [release status](release-status.md).

WSL registration follows [Microsoft's import interface](https://learn.microsoft.com/en-us/windows/wsl/basic-commands#import-a-distribution) and [distribution VHD location](https://learn.microsoft.com/en-us/windows/wsl/disk-space#how-to-locate-the-vhdx-file-and-disk-path-for-your-linux-distribution).

## 中文

Windows 客户端包含干净的 WSL 后端根文件系统，内置 Rust 服务、网页、Docker/Compose、数据库及桌面镜像、ADB 与媒体协议。工厂使用 TypeScript，实际初始化、控制、渠道和媒体由 Rust 负责。

运行数据留在 `/var/lib/aiphone`，系统包不包含账号、聊天或设备盘。客户端先初始化 WSL，再启动原生 Android 并建立配对；后端启动回执与双设备就绪回执分开，不能相互替代。

现有启动、重启和持久化验收由隔离容器内的 Rust worker 执行，外层驱动管理所属资源与回执。当前迁移候选尚未集中验证；Windows 实机、WHPX/防火墙、完整首启及升级资格分别记录。
