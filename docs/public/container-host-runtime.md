# Offline container host

The Apple Silicon client includes Lima with Virtualization.framework, a pinned Alpine image and offline Docker/Compose packages. The engine uses an installation-owned Docker context. Windows runs its [QEMU Linux guest](windows-backend-runtime.md).

## Assembly

`scripts/tooling/container-host.ts` assembles the pinned archive, native Lima binary, local system image and packages into a versioned component. The [Mac build guide](engineering/macos-full-runtime.md) describes complete-client assembly. Dependency builds run on disposable Linux workers.

## Runtime ownership

The engine mounts its installation directory through virtiofs. Personal SSH keys, SSH-agent forwarding and proxy propagation are disabled. Provisioning uses local packages with `apk --no-network`. Persistent engine state belongs to the installation and is retained across restarts; the engine does not change the global Docker context or create a public endpoint.

Android acceleration, two-device readiness, media and upgrades have separate acceptance requirements. See [release status](release-status.md) and [distribution requirements](distribution-requirements.md). Binary distribution requires matching notices and applicable corresponding source.

## 中文

Apple Silicon 客户端内置 Lima/VZ 引擎、Alpine 镜像和离线 Docker/Compose 软件包。引擎使用应用自己的 Docker context。Windows 使用 QEMU Linux 客体。

引擎数据归属于当前安装，重启时复用。完整设备就绪、媒体、更新与发行资格分别记录，详见[发行状态](release-status.md)。
