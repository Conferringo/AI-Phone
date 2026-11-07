# Offline container host

The Apple Silicon client includes its own Lima engine using Virtualization.framework, a pinned Alpine image and the offline Docker/Compose package closure. The client initializes these resources without adopting the user's Docker context or installing tools globally. Windows uses its separate [WSL backend](windows-backend-runtime.md).

## Assembly

`scripts/tooling/container-host.ts` assembles the pinned archive, native Lima binary, local system image and packages into a versioned component. The [Mac build guide](engineering/macos-full-runtime.md) describes complete-client assembly. Dependency builders run in disposable Linux build environments; those builders are not a Linux client distribution.

## Runtime ownership

The engine mounts its installation directory through virtiofs. Personal SSH keys, SSH-agent forwarding and proxy propagation are disabled. Provisioning uses local packages with `apk --no-network`. Persistent engine state belongs to the installation and is retained across restarts; the engine does not change the global Docker context or create a public endpoint.

Android acceleration, two-device readiness, media and upgrades have separate acceptance requirements. See [release status](release-status.md) and [distribution requirements](preview-release.md). Binary distribution requires matching notices and applicable corresponding source.

## 中文

Apple Silicon 客户端内置 Lima/VZ 引擎、本地 Alpine 镜像和离线 Docker/Compose 软件包；不采用用户的 Docker context，不向宿主全局安装工具。Windows 使用独立的 WSL 后端。Linux 构建环境只用于组件生产，设备里的 Linux 电脑继续保留。

引擎数据归属于当前安装，重启时复用。完整设备就绪、媒体、更新与发行资格分别记录，详见[发行状态](release-status.md)。
