# Windows backend runtime

The Windows backend factory assembles an offline Linux root filesystem for an
application-owned WSL2 distribution. It contains prebuilt services and a desktop
environment; end users do not compile a backend or prepare its container images.
Windows distribution import, reboot recovery and the native Android bridge are
separate integration boundaries and are not enabled by this component alone.

## Contents and ownership

| Resource | Owner and contract |
| --- | --- |
| Ubuntu root filesystem | Immutable base digest, original notices and a bounded archive inventory |
| Python, Node, web, native services and OCR | Host-matched component manifests; source and file digests verified before assembly |
| Docker Engine and Compose | Pinned Alpine dependency closure in an isolated musl prefix; private Unix socket and client state |
| Database, cache, workspace and desktop images | Four-role sealed image capsule; no build or pull during initialization |
| User state | Created under `/var/lib/aiphone` at runtime; excluded from the distributed rootfs |

The backend does not import the host's Docker context or Lima state. It owns
`unix:///run/aiphone/docker.sock` and its CLI configuration. Android SDK parsing is
lazy: an unavailable phone returns a device error while desktop media can start.
The Windows host remains responsible for native Android acceleration and control.

## Factory and qualification

[`wsl-backend.yml`](../../.github/workflows/wsl-backend.yml) runs on each pull
request and can also be dispatched explicitly. The disposable Linux factory
builds the precompiled components, assembles and
inspects the rootfs, then boots that same filesystem in an owned test container.
The nested-engine test uses a random identity and volume, with no host source,
Docker socket or device mounts.

Qualification starts the bundled engine, PostgreSQL, Redis, assistant, web,
channels, media and desktop. It registers a test desktop through protobuf,
checks an authenticated web media frame, then starts the same isolated state a
second time to verify a work file and database marker. Normal completion requires
all checks and owned-resource cleanup to succeed. The workflow retains only a
JSON receipt; images, logs, credentials and guest data are not uploaded.

The component records `windows_boot_verified=false`, `phone_ready=false` and
`complete_device_bundle=false`. Linux qualification is not a Windows WSL/WHPX
installation result. The Windows independent-host policy remains disabled until
that integration has its own evidence. Binary redistribution is reviewed through
the [release requirements](preview-release.md) and [third-party notices](../../THIRD_PARTY_NOTICES.md).

Contributor checks use the project-local Bun and isolated fixtures:

```sh
./scripts/bootstrap.sh
PYTHONPATH=scripts python3 -m unittest test_wsl_backend test_wsl_backend_review test_wsl_runtime test_native_services.NativeHostToolsTests
./scripts/bun test ./apps/device-stream ./scripts/wsl-desktop-qualification.test.ts
```

Full qualification requires the dedicated workflow and its actual capacity
budget; these unit checks do not start a personal device or qualify Windows boot.

## Windows 后端

该组件为应用专属 WSL2 环境生成离线 Linux 根文件系统，包含预编译后端、工具和四类
容器镜像。用户数据在初始化时单独创建，不进入发行镜像。Docker 使用独立 socket 与
配置，电脑媒体服务不依赖手机 SDK 已经就绪。

专用 Linux 工厂会实际检查后端、认证网页画面，以及两次启动间的工作文件和数据库
保留；结果只保存为 JSON 回执。该验证不等同于 Windows 安装通过。WSL 导入、系统
重启恢复、WHPX 和原生 Android 桥仍须独立接通及验收，完整发行状态见
[发行范围](release-status.md)。
