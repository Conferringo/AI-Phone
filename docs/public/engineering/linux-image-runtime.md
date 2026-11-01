# Linux desktop and service image component

The native installer can carry a host-architecture-matched `images.payload` alongside Android and source components. It contains four clean image roles: the Agent work environment, Linux GUI desktop, PostgreSQL and Redis. User homes, database volumes, browser profiles, phone disks, credentials and chats remain separate installation state. A running user's container is never an input.

The desktop contains Openbox, Chromium, LibreOffice, Chinese fonts/input support, audio and notification helpers, and the pinned Clash application. This is a container filesystem, **not a complete hypervisor appliance**. The container host's kernel/VM, engine and other host dependencies remain separate distribution work.

## Validation and activation

New capsules use format 2: the outer tar contains `images.tar.xz` and `IMAGES-MANIFEST.json`. Format 1 capsules containing `images.tar.gz` remain readable. Import streams the verified compressed body into `docker image load`, without another image-sized staging copy.

The maintainer exporter first seals and inspects a fresh Docker export. For OCI layouts, workspace/desktop gzip filesystem layers are represented by their exact uncompressed TAR bytes before outer XZ compression. Configuration bytes and root filesystem DiffIDs stay unchanged; PostgreSQL, Redis and proof-reachable objects retain their original bytes. Changed local manifests and references are rebound together. Docker Schema 2 image descriptors are supported, and valid classic exports pass through without changing their raw TAR. The exporter temporarily needs both compressed representations; the installation path does not.

The manifest binds architecture, archive size/digest, image configuration and exported platform manifest. References use `ai-phone-bundle/<role>:sha-<platform-manifest-sha256>`. A multi-platform source index is provenance, not proof of the selected platform. OCI configuration bindings and the entire alias inventory are verified. Unnamed upstream attestations may refer only to exported subjects; foreign named images are rejected.

Admission limits are 8 GiB of archive/file bodies, 20,000 nested headers, 8 MiB of retained JSON metadata in aggregate, and a 4 GiB recovery reserve. Paths must be canonical and relative; symlinks and legacy aliases are rejected. A conflicting existing reference stops initialization instead of being overwritten.

Activation rechecks the capsule digest and uses one open file descriptor through import. An atomic application update cannot redirect that import to a replacement file. A failed or uncertain load publishes no selection; a retry observes all images first. Only verified images become the atomically saved private `data/deployment/image-runtime.json`. Compose and the Agent receive these references with `pull_policy=never`. A missing bundled desktop fails initialization instead of triggering a download or source build.

These JSON records describe local artifacts and installation state. Browser business requests remain protobuf.

## Maintainer workflow

Use a clean source export and a dedicated builder engine. Build the workspace and desktop Dockerfiles with `infra/workspace` and `infra/desktop` as their contexts, never a user's installation root. The desktop's `clash_apps` named context must contain the matching verified package from `config/clash-apps.json`. Its installer is mounted during the build step rather than retained in an image layer.

Review the built images and digest-pinned PostgreSQL/Redis defaults in `compose.yaml`. Record their references in a local JSON object with keys `workspace`, `desktop`, `postgres`, `redis`, then run:

```sh
python3 scripts/build_container_images.py \
  --sources /path/to/reviewed-images.json \
  --output /path/to/images.payload

python3 scripts/build_client.py --platform macos \
  --with-web-runtime --with-service-runtime \
  --android-runtime /path/to/android.payload \
  --images-runtime /path/to/images.payload
```

This is a maintainer workflow. Complete product users should receive the components inside the distribution and configure devices and a model in the client. Source-only builds exclude optional capsules. Preparing a capsule does not approve binary publication: licenses, notices, corresponding source and all other distribution gates remain independent.

## Availability

Current platform support, installer availability and distribution requirements are maintained in [release status](../release-status.md). Component formats and build interfaces do not independently qualify a complete installation.

The component remains subject to complete host/media integration and third-party redistribution requirements. See the [Android component](android-runtime.md) for its separate resource contract.

---

## 中文

Linux 桌面、Agent 工作环境、PostgreSQL 和 Redis 采用独立镜像组件随客户端部署。仅使用干净源码和固定基础镜像；用户设备盘、主目录、数据库卷、账号与聊天保持分离。这个组件提供容器文件系统，尚不等于完整虚拟化宿主、内核或四平台独立安装包。

安装前验证架构、SHA-256、OCI 配置绑定和镜像别名，按平台内容摘要命名，冲突即停止。流式导入的未知结果先观察，全部验证后才原子发布私有选择记录。正式选择禁止自动拉取或源码构建；归档、文件数量、累计元数据和恢复空间均有保护。

镜像资源与完整宿主、媒体输入输出及平台初始化分别配置和验收。当前安装包供应、平台范围及第三方再分发要求见[发行状态](../release-status.md)。
