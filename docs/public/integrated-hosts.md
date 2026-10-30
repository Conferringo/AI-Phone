# Self-contained host runtime

The product target is an installed client that creates its phone, computer and model configuration without a manual server deployment. **Complete distributions are not available yet.** Source bootstrap and connection clients are separate developer/remote-access paths, not substitutes for the complete product. See the [release contract](release-contract.en.md) and [current status](release-status.md).

## Platform scope

| Host | Current source path | Unfinished independent-host work |
|---|---|---|
| macOS Apple Silicon only | Native first-run worker, versioned resources, prebuilt web/services/tools/phone applications, Android and Linux image components; local Lima/VZ host component | Full assembled client, remaining offline media/channel/browser dependencies, clean default-device creation and full recovery |
| Windows x64 | Native client and explicit local/LAN connection | Complete WHPX/WSL2 runtime, matched images/services, independent initialization and supervision |
| Linux x64 / ARM64 | Native clients; [x64 managed source installer](managed-linux.md) | Offline distribution, matched components, KVM/system integration and clean-host acceptance; ARM64 independent host remains incomplete |
| Android ARM64 | Native client | Root/KVM host runtime, device matrix, complete privileged dual-device deployment; ordinary unrooted devices are outside this host scope |

A successful client compilation is not independent-host acceptance. Physical non-macOS installation tests are waived and remain unverified; component completeness, build evidence and redistribution review are still required.

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

Android clean-base boot, root, frames/input and restart persistence have separate diagnostic evidence. Linux desktop, audio capture and local image import have separate evidence. The offline macOS engine has real VM/container startup evidence. None of these proves that the final installer has created both default-sized devices, configured a model and completed the whole workflow on a clean host. The remaining media/channel/browser dependencies and redistribution review must still be completed.

---

# 一体化宿主运行时

目标是下载安装客户端后，直接配置手机、电脑和模型，不要求用户手动部署服务。**完整发行包尚未完成**；源码引导和连接客户端是独立的开发/远程使用路径。macOS仅支持M系列芯片，Windows、Linux和Android范围不变；Android独立宿主优先root/KVM。

完整包应自带系统镜像、模拟器、服务与私有工具，不让普通用户安装SDK、编译器或Docker。能容器化的依赖放入本项目容器，原生虚拟化/输入/视觉程序放在应用自己的目录，不修改用户全局开发环境。完整macOS包优先自己的Lima/VZ引擎，不改全局Docker context；代码版本、持久user目录和短路径容器状态分别归属同一安装，重开恢复原环境。

首次安装需要至少12GiB内存、35GiB可用存储，推荐16GiB内存。设备资源在创建时设置，已有设备不会随预设变化被重建或扩容。关闭、中断、重试、缺失组件和恢复隔离均要明确反馈；单项测试或阶段完成不能代替完整初装验收。现有设备、聊天、账号和密钥不进入源码或运行组件。
