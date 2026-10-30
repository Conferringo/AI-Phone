# Linux desktop and service image component

The native installer can carry a host-architecture-matched `images.payload` alongside Android and source components. It contains four clean image roles: the Agent work environment, Linux GUI desktop, PostgreSQL and Redis. User homes, database volumes, browser profiles, phone disks, credentials and chats remain separate installation state. A running user's container is never an input.

The desktop contains Openbox, Chromium, LibreOffice, Chinese fonts/input support, audio and notification helpers, and the pinned Clash application. This is a container filesystem, **not a complete hypervisor appliance**. The container host's kernel/VM, engine and other host dependencies remain separate distribution work.

## Validation and activation

The outer tar contains `images.tar.gz` and `IMAGES-MANIFEST.json`. Export streams Docker's archive directly through compression into the capsule; import streams its verified body into `docker image load`, without another image-sized staging copy.

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

## Evidence and limits · 2026-10-30

- The ARM64 development capsule is 1,225,004,032 bytes. Existing user containers and volumes were not exported or restarted.
- An installer process denied outbound IP by an OS sandbox restored the new references through Docker's Unix socket, without registry pulls or builds. The shared Docker daemon itself was not globally disconnected.
- A disposable, network-disabled desktop without host mounts returned a 1280×800 screen and exact native pointer coordinates, saved Chinese text, recorded a notification and generated a spreadsheet. Chromium and Clash were present.
- Four real media checks passed: daemon crash recovery, disconnect cleanup, heartbeat expiration, and a played tone producing nonzero PCM at the host capture pipe. Browser autoplay and physical-speaker acceptance remain pending.
- Isolated regressions cover validation, partial imports, conflicting references, package replacement and resource budgets. The three new packaging modules have CC ≤9, CRAP <10 and no duplication candidates at 0.8. A frozen-source run detected 179 of 183 selected mutations; four survived (a stricter minimum outer-member size and fixed metadata ordering/epoch variants). They are recorded rather than counted as detected. These checks do not close the repository-wide architecture audit.

The capsule remains `review-pending`; no complete product installer has been released. ARM64 evidence does not validate x64 images, Windows supervision, Android root/KVM hosts or a full offline first run. See [release status](../release-status.md) and [Android component](android-runtime.md).

---

## 中文

Linux 桌面、Agent 工作环境、PostgreSQL 和 Redis 采用独立镜像组件随客户端部署。仅使用干净源码和固定基础镜像；用户设备盘、主目录、数据库卷、账号与聊天保持分离。这个组件提供容器文件系统，尚不等于完整虚拟化宿主、内核或四平台独立安装包。

安装前验证架构、SHA-256、OCI 配置绑定和镜像别名，按平台内容摘要命名，冲突即停止。流式导入的未知结果先观察，全部验证后才原子发布私有选择记录。正式选择禁止自动拉取或源码构建；归档、文件数量、累计元数据和恢复空间均有保护。

ARM64 实际完成受限网络进程的镜像导入，以及隔离桌面的画面、光标、中文、通知、浏览器、Clash、表格验证。音频实测包含真实信号到达宿主捕获管道；网页自动播放和物理扬声器仍待验收。再分发为 review-pending，完整容器宿主、媒体/渠道运行环境和各平台初始化继续推进，不以局部通过代替产品完成。
