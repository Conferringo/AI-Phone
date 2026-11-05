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
| Guest phone tools | Pinned Linux x64 platform-tools, upstream notices and a vendored AOSP media schema; no nested emulator or system image |
| User state | Created under `/var/lib/aiphone` at runtime; excluded from the distributed rootfs |

The backend does not import the host's Docker context or Lima state. It owns
`unix:///run/aiphone/docker.sock` and its CLI configuration. Android SDK parsing is
lazy: an unavailable phone returns a device error while desktop media can start.
The Windows host remains responsible for native Android acceleration and control.

Source expansion uses the shared Rust resource publisher. The factory extracts
the source-bound native component, verifies its staging capability and includes
the exact source digest in the rootfs inventory. Both verification and first-run
initialization use that component to check, stage or resume the source tree and
its persistent data links. The legacy Python entry consumes the native receipt;
it no longer publishes WSL source trees itself. Configuration, engine/image
activation and service-start orchestration still require Python, so this is a
partial runtime migration in newly built root filesystems. Registered guests keep
their installed bootstrap; in-place backend upgrades remain separate work.

The [Rust phone/backend bridge](engineering/device-bridge.md) supplies paired ADB and emulator-gRPC transport. Its startup receipt is separate from phone readiness. Newly built root filesystems include `/opt/aiphone/native-phone-bridge`, the SDK root already used by the backend profile. The maintainer downloads platform-tools using the existing Android factory pin; assembly then verifies and copies the offline resource, including its version, file digests and notices. The rootfs factory executes the packaged Linux ADB version command without network access. Archive admission requires ADB and the media schema and permits only that schema under the `emulator` directory, never an emulator executable.

The source-provenance record identifies the exact vendored AOSP schema; it is not claimed byte-identical to every SDK release. Maintainer checks exercise the existing discovery/media consumer against an actual Android. The [native phone lifecycle](engineering/device-bridge.md#native-lifecycle-and-automatic-pairing) now starts and holds the paired guest after backend initialization. This resource slice alone does not provision the phone or establish complete readiness. Older backend capsules must be rebuilt for the new resource inventory. Existing registered guests are preserved; an in-place upgrade is not supplied by this resource factory.

## Native client resources

Maintainer builds accept `--wsl-runtime /factory/backend.payload` for Windows. The resource preparer validates the sealed capsule and materializes its exact `rootfs.tar.gz` and manifest under the client's `host/wsl` directory. A compiled manifest digest binds that group to the executable. Artifact inspection rejects changed or additional group members. Source-only builds omit the group; end users do not run this preparation step.

The Rust client passes the installed resource directly to the application-owned WSL lifecycle. When an Android component is also present, [native Android preparation](engineering/native-android-preparation.md) validates it and creates or recovers the phone configuration under the same installation lock. The first WSL import checks its image digest and capacity; a registered distribution resumes from its owned disk without another base-image copy or import. The initializer sends the bounded first-run request through stdin to the bundled backend entry. A successful terminal acknowledgement marks only the backend initialized. The native phone worker then checks the peer capability, boots or observes the same phone and holds its host/guest bridge independently of the UI. Interrupted execution remains unknown until observed; it does not reimport or erase the guest. Complete device provisioning and installed Windows qualification remain pending, so the complete-install checkpoint and independent Windows deployment stay disabled.

## Native registration

The Rust host runtime owns rootfs admission, WSL2 registration and registration
recovery. It verifies the base-image digest once before a new import and records
the application-owned directory before invoking `wsl.exe`. Registry reads use
native Windows APIs through Microsoft's `windows-registry` crate. Recovery binds
the distribution name, WSL version and actual registered directory to the saved
owner record.

| Saved state | Recovery |
| --- | --- |
| Prepared | Verify the base image before the first import |
| Importing | Observe the registered directory; an absent or uncertain result is not replayed |
| Registered | Reuse the existing guest without reimporting or rereading its base image |

The registration stage preserves existing guest disks and rejects a changed base
image or a foreign registration. It does not unregister distributions, change the
default distribution, modify global WSL settings or shut down other environments.
Native command cancellation waits for the owned process and its bounded output
readers; an interrupted OS operation remains subject to registration recovery.

`wsl-host import <rootfs> <installation> <sha256> <expanded-bytes>` is the native
installer component interface, not an end-user deployment procedure. Its result
keeps `backend_initialized=false` and `phone_ready=false`. The separate
`start-bundle` component initializes the existing backend through stdin and accepts
only a successful bounded terminal acknowledgement. It retains `phone_ready=false`.
The initializer still uses Python beyond the Rust source-publication step;
the remaining migration, WSL feature enablement, the native Android bridge and live
first-run readiness remain required before Windows independent deployment is enabled.
The Windows CI job compiles this component and exercises its registry adapter;
those checks do not establish a real WSL/WHPX guest installation.

The command boundary follows [Microsoft's WSL import interface](https://learn.microsoft.com/en-us/windows/wsl/basic-commands#import-a-distribution).
Registration directory identity follows the [documented VHD location lookup](https://learn.microsoft.com/en-us/windows/wsl/disk-space#how-to-locate-the-vhdx-file-and-disk-path-for-your-linux-distribution).

## Factory and qualification

The backend factory can also embed the two phone application groups with
`python3 scripts/wsl_backend_qualification.py --with-device-apps`. This maintainer
path compiles the input method, launcher and notification probe, then collects
the pinned Clash APKs using the existing application producers. It requires
Android 35 compiler tools and Java 17 on the disposable build host. Admission
accepts either the three backend components or those components with both phone
application groups; partial groups and a nested container host are rejected.
The additional resources do not establish native phone, transport or complete
Windows first-run readiness. Both qualification modes now include the guest phone tools; only the optional mode carries the two phone application groups.

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

Contributor setup and automated checks are described in [Contributing](../../CONTRIBUTING.md).

## Windows 后端

该组件为应用专属 WSL2 环境生成离线 Linux 根文件系统，包含预编译后端、工具和四类
容器镜像。用户数据在初始化时单独创建，不进入发行镜像。Docker 使用独立 socket 与
配置，电脑媒体服务不依赖手机 SDK 已经就绪。

专用 Linux 工厂会实际检查后端、认证网页画面，以及两次启动间的工作文件和数据库
保留；结果只保存为 JSON 回执。该验证不等同于 Windows 安装通过。Rust 已接通专属 WSL 注册与客体后端初始化，
通过标准输入传递配置并核对有界终态回执；中断后保留未知结果与原设备盘，不重复导入。
源码展开、完整性核对、恢复和持久数据链接已复用 Rust 资源层；配置、镜像激活与服务
编排仍用 Python，不能称纯 Rust 完成。剩余迁移、系统重启恢复、WHPX、原生 Android 桥与
真实双设备就绪仍未通过，Windows 独立部署保持关闭，完整发行状态见
[发行范围](release-status.md)。
