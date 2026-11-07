# Self-contained host runtime

The product target is an installed client that creates its phone, computer and model configuration without a manual server deployment. Source bootstrap and connection clients are separate developer/remote-access paths, not substitutes for the complete product. See the [distribution requirements](preview-release.md) and [current installer availability](release-status.md).

## Platform scope

The [release-status platform table](release-status.md#platform-qualification) owns current implementation and qualification. The architecture differs by host:

- **macOS Apple Silicon:** native ARM64 Android Emulator and an app-owned Lima/VZ container host. Intel Macs are outside the supported target.
- **Windows x64:** the intended host combines native Android Emulator/WHPX with a WSL2 Linux backend. Windows must own distro initialization, phone/backend bridging and service recovery; those boundaries cannot be inferred from a Linux component build.
- **Linux:** the x64 source path uses Android Emulator/KVM and an existing Docker engine or app-owned Lima/QEMU host. ARM64 needs architecture-matched components.

Physical installation tests are waived for previews on Windows, Apple Silicon macOS and Linux and remain unverified; component completeness, build evidence and redistribution review are still required.
The default guest templates are Android 15 and Debian 13/Openbox. Guest CPU, memory and initial phone storage are configurable. Existing device storage is never silently resized by changing a creation preset. The Linux desktop shares its container host kernel; a separate-kernel Fedora desktop is a future adapter.

## Installation and recovery contract

1. Choose language and device resources in the installed client. A fresh installation requires at least 12 GiB RAM and 35 GiB free storage; 16 GiB RAM is recommended.
2. The complete distribution must carry its matched images, emulator, services and private tools. It must not ask ordinary users to install Rust, Java, Node, Python, Docker or download system images. Required operating-system permissions and distribution terms remain explicit.
3. Stage verified code under `host/releases/<payload hash>`. Keep devices, database state, files and configuration under `host/user`; an update does not overwrite them.
4. Create the devices and start owned services; verify real frames and service health before entering the existing Agent model form. Completed phases support retry but do not prove that the whole deployment is ready.
5. On reopening, restore the selected runtime environment and start stopped owned services without restarting active work. A missing or changed runtime is a recoverable error, not permission to switch silently to another installation.

Stopping installation terminates its child process group and retains completed work. SDK license prompts are never automatically accepted from a saved pending prompt. Foreign jobs/containers and quarantined restored data are rejected before startup changes. This is not full upgrade rollback or a signed updater.

Initial profile creation, validation and explicit edits now use the qualified Rust host-profile component. It preserves existing device resources, credentials and staged user-directory links. The installer and contributor CLI delegate to that component; remaining Python orchestration is still scheduled for retirement. See [private installation state](engineering/private-install-state.md).

Runtime environment selection now also belongs to Rust: saved PATH ownership, SDK/Java selection, guest ABI/resources, device proxies, installed image references and guest-app metadata share one read-only entry. WSL consumes that same profile policy while retaining its fixed Docker socket and process environment. Installer tool preparation and runtime selection use a shared native component implementation. Existing Python callers retain a thin command adapter; the complete macOS/Linux initializer now runs in Rust and its full factories omit portable Python/Node. WSL factory verification and repository tools still use Python. Fresh installation qualification and publication remain separate.

The Windows adapter imports the owned WSL bundle and invokes its Rust backend initializer through a bounded stdin configuration stream. It does not mount Windows drives or put configuration in process arguments. Backend acknowledgement sets only `backend_initialized`. The native client then starts the owned Android/bridge worker and completes the durable device setup and service-readiness check before reporting ready. Cancellation after launch leaves unknown guest results explicit and retains the registered distribution. The qualification driver now invokes that same native initializer. The complete EXE, WSL2/WHPX and first-run runtime qualification remain pending; factory/qualification tooling and other runtime consumers still need Python retirement.

## Isolation

A complete macOS bundle selects its own [container-host component](container-host-runtime.md), even when another Docker engine is running. It uses a real short private Lima state directory bound to this installation's persistent user data; executable selection is restored after reopening. It never changes global Docker contexts. Contributor source mode can reuse an explicitly available engine, but must still check container ownership.

Tools stay inside the installation, and backend/development dependencies belong inside its containers where practical. Native virtualization and input/vision helpers remain app-private platform binaries. Guest Clash applications control the guests' networks; the host defaults to direct networking. User proxy subscriptions are never bundled.

## Evidence limits

The Unix client entry now invokes the Rust initializer rather than `entry.py` or a downloaded mise/Python toolchain. It stages bundled source and Android resources, preserves the saved device profile, starts the owned offline container host and backend, boots the native phone owner, installs input/notification and guest applications, and runs native device setup before emitting ready. Missing components fail explicitly. Local controller, CLI and Tauri tests cover this source change; a complete cold installation with both devices has not yet been qualified. Desktop network directory preparation is native, and its former Python launcher is retired. Remaining Python runtime/maintenance calls and JavaScript network/channel/media services still need migration.

Component execution, package inspection and complete installation are separate checks. Booting an Android image, displaying a desktop or starting an offline container host does not establish that a final installer can create both default-sized devices, configure a model and complete the whole workflow on a clean host. The [release status](release-status.md) records their verified scopes and remaining qualification. Redistribution permission and corresponding-source obligations apply even when a package builds successfully.

---

# 一体化宿主运行时

Unix客户端入口现调用Rust初始化器，移除原entry.py及下载mise/Python工具链路径。初始化串联包内源码/Android资源、保存的设备配置、自有离线容器宿主、数据库与服务、原生手机owner、输入/通知及客体应用安装，最后通过原生设备创建与画面检查才发出ready；缺少组件直接报错。本地控制器、CLI及Tauri测试覆盖这次源码改动，完整双设备冷安装尚未通过。桌面网络目录准备已原生化并退役Python跳板；其余Python运行/维护调用和网络/渠道/媒体JavaScript服务仍须继续迁移。

目标是下载安装客户端后，直接配置手机、电脑和模型，不要求用户手动部署服务。安装包供应、已有组件和各平台待验收项统一见[发行状态](release-status.md)；源码引导和连接客户端是独立的开发/远程使用路径。macOS 仅支持 M 系列芯片；Windows 目标组合原生 Android/WHPX 与 WSL2 后端；Linux 使用匹配架构的 Android/KVM 与容器宿主。

完整包应自带系统镜像、模拟器、服务与私有工具，不让普通用户安装SDK、编译器或Docker。能容器化的依赖放入本项目容器，原生虚拟化/输入/视觉程序放在应用自己的目录，不修改用户全局开发环境。完整macOS包优先自己的Lima/VZ引擎，不改全局Docker context；代码版本、持久user目录和短路径容器状态分别归属同一安装，重开恢复原环境。

首次安装需要至少12GiB内存、35GiB可用存储，推荐16GiB内存。设备资源在创建时设置，已有设备不会随预设变化被重建或扩容。关闭、中断、重试、缺失组件和恢复隔离均要明确反馈；单项测试或阶段完成不能代替完整初装验收。现有设备、聊天、账号和密钥不进入源码或运行组件。

Windows适配器已接通所属WSL资源导入和Rust后端初始化，配置经有界标准输入传入，不挂载Windows磁盘、不写入进程参数。后端回执只确认初始化；原生客户端随后启动所属Android/桥接worker，完成持久设备配置与服务就绪检查后才进入ready。启动后取消保留明确的未知结果及原发行实例。验收驱动已调用同一原生命令；完整EXE、WSL2/WHPX及首次运行资格仍待验证，工厂/验收工具及其他运行消费者的Python退役尚未完成。

运行环境选取也已迁Rust，统一保存的PATH、SDK/Java、客体ABI/资源、设备代理、镜像引用与客体应用元数据。WSL复用同一配置策略，继续保留其固定Docker套接字和进程环境；工具组件的安装准备与运行选择共享原生实现。旧Python调用仅保留薄命令适配，macOS/Linux完整初始化现由Rust执行，其完整工厂不再携带portable Python/Node；WSL工厂校验与仓库工具仍需继续退役Python，完整初装验收与发行资格另列。
