# Self-contained host runtime

The product target is an installed client that creates its phone, computer and model configuration without a manual server deployment. Source bootstrap and connection clients are separate developer/remote-access paths, not substitutes for the complete product. See the [release contract](release-contract.en.md) and [current installer availability](release-status.md).

## Platform scope

The [release-status platform table](release-status.md#platform-qualification) owns current implementation and qualification. The architecture differs by host:

- **macOS Apple Silicon:** native ARM64 Android Emulator and an app-owned Lima/VZ container host. Intel Macs are outside the supported target.
- **Windows x64:** the intended host combines native Android Emulator/WHPX with a WSL2 Linux backend. Windows must own distro initialization, phone/backend bridging and service recovery; those boundaries cannot be inferred from a Linux component build.
- **Linux:** the x64 source path uses Android Emulator/KVM and an existing Docker engine or app-owned Lima/QEMU host. ARM64 needs architecture-matched components.
- **Android ARM64:** the independent-host target requires root and exposed KVM. A normal unrooted APK is outside this scope; restricted AVF APIs do not provide arbitrary dual-guest hosting.

Physical non-macOS installation tests are waived and remain unverified; component completeness, build evidence and redistribution review are still required.
The default guest templates are Android 15 and Debian 13/Openbox. Guest CPU, memory and initial phone storage are configurable. Existing device storage is never silently resized by changing a creation preset. The Linux desktop shares its container host kernel; a separate-kernel Fedora desktop is a future adapter.

## Installation and recovery contract

1. Choose language and device resources in the installed client. A fresh installation requires at least 12 GiB RAM and 35 GiB free storage; 16 GiB RAM is recommended.
2. The complete distribution must carry its matched images, emulator, services and private tools. It must not ask ordinary users to install Rust, Java, Node, Python, Docker or download system images. Required operating-system permissions and distribution terms remain explicit.
3. Stage verified code under `host/releases/<payload hash>`. Keep devices, database state, files and configuration under `host/user`; an update does not overwrite them.
4. Create the devices and start owned services; verify real frames and service health before entering the existing Agent model form. Completed phases support retry but do not prove that the whole deployment is ready.
5. On reopening, restore the selected runtime environment and start stopped owned services without restarting active work. A missing or changed runtime is a recoverable error, not permission to switch silently to another installation.

Stopping installation terminates its child process group and retains completed work. SDK license prompts are never automatically accepted from a saved pending prompt. Foreign jobs/containers and quarantined restored data are rejected before startup changes. This is not full upgrade rollback or a signed updater.

## Isolation

A complete macOS bundle selects its own [container-host component](container-host-runtime.md), even when another Docker engine is running. It uses a real short private Lima state directory bound to this installation's persistent user data; executable selection is restored after reopening. It never changes global Docker contexts. Contributor source mode can reuse an explicitly available engine, but must still check container ownership.

Tools stay inside the installation, and backend/development dependencies belong inside its containers where practical. Native virtualization and input/vision helpers remain app-private platform binaries. Guest Clash applications control the guests' networks; the host defaults to direct networking. User proxy subscriptions are never bundled.

## Evidence limits

Component execution, package inspection and complete installation are separate checks. Booting an Android image, displaying a desktop or starting an offline container host does not establish that a final installer can create both default-sized devices, configure a model and complete the whole workflow on a clean host. The [release status](release-status.md) records their verified scopes and remaining qualification. Redistribution permission and corresponding-source obligations apply even when a package builds successfully.

---

# 一体化宿主运行时

目标是下载安装客户端后，直接配置手机、电脑和模型，不要求用户手动部署服务。安装包供应、已有组件和各平台待验收项统一见[发行状态](release-status.md)；源码引导和连接客户端是独立的开发/远程使用路径。macOS 仅支持 M 系列芯片；Windows 目标组合原生 Android/WHPX 与 WSL2 后端；Linux 使用匹配架构的 Android/KVM 与容器宿主；Android 独立宿主要求 root 和开放 KVM。

完整包应自带系统镜像、模拟器、服务与私有工具，不让普通用户安装SDK、编译器或Docker。能容器化的依赖放入本项目容器，原生虚拟化/输入/视觉程序放在应用自己的目录，不修改用户全局开发环境。完整macOS包优先自己的Lima/VZ引擎，不改全局Docker context；代码版本、持久user目录和短路径容器状态分别归属同一安装，重开恢复原环境。

首次安装需要至少12GiB内存、35GiB可用存储，推荐16GiB内存。设备资源在创建时设置，已有设备不会随预设变化被重建或扩容。关闭、中断、重试、缺失组件和恢复隔离均要明确反馈；单项测试或阶段完成不能代替完整初装验收。现有设备、聊天、账号和密钥不进入源码或运行组件。
