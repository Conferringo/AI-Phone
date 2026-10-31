# Managed Linux host

The source installer provides a client-owned Linux x64 setup path for Ubuntu/Debian. This guide covers the online development bootstrap; it downloads tools and builds services. The separate [Linux package factory](linux-client-factory.md) assembles bundled resources. Current installer availability and host qualification are maintained in [release status](release-status.md).

The native client selects language, phone CPU/RAM/storage and desktop CPU/RAM. It downloads checksum-pinned tools, chooses an Android 15 **x86_64** image, prepares its own AVD and guest applications, starts the database/backend/media/workbench and hands off to model configuration. There are no user-written service deployment commands in this path.

## Host requirements and authorization

- x64 Ubuntu/Debian, at least 12 GiB RAM and 35 GiB initially free; 16 GiB RAM is recommended.
- Hardware virtualization and an accessible `/dev/kvm`. Missing firmware/kernel support is a real blocking error; the installer does not silently substitute unusably slow software emulation.
- Distribution dependencies are installed through the OS's PolicyKit dialog when necessary. Passwords are not collected by AI Phone. KVM permission can be granted to the current user on the current device; no broad Docker group or global security-policy change is made.
- A usable existing Docker engine may be reused. Otherwise an app-owned Lima/QEMU container host is downloaded and provisioned, mounting only the app installation directory. No global Docker context is changed. Linux virtiofs remains an upstream experimental path and needs acceptance on the actual distribution.

## Lifecycle and evidence

Linux supervisors have one exclusive lock per service. Reopening setup reuses the same live service specification. Changed specifications or unknown ownership are rejected before replacing a running task. A process receipt includes its `/proc` start identity, UID and supervisor command; a reused PID never authorizes termination. Each supervisor restarts only its own child process. Stopping retains guest disks, chats, credentials and database files.

Service startup does not compile over a live web/runner/assistant process. On resume, Android readiness is observed without reapplying its network/home/input settings. The two-device ready gate requires a real Android PNG, root identity and a real desktop PNG. HTTP readiness alone is insufficient.

Real isolated Linux regressions cover preserved PIDs, child SIGKILL/restart, changed-command rejection and a forged old PID receipt. The `Managed Linux host acceptance` workflow uses the same source payload and Python entry point as the native installer, with both existing-engine and managed-engine cases. It only copies SDK license files already accepted in the hosted CI image; a new license prompt fails rather than auto-accepting. No private guest files or raw token-bearing logs are uploaded.

Host architecture differs on Windows and Android; see [integrated-host scope](integrated-hosts.md#platform-scope). Linux process regressions do not establish another platform's initialization, phone/backend bridge or dual-device lifecycle.

---

# Linux 客户端本机安装

源码安装器提供 Ubuntu/Debian x64 本机安装路径。本文介绍需要下载工具和编译服务的在线开发引导；包含预构建资源的发行包由独立的[Linux 包工厂](linux-client-factory.md)组装。安装包供应与各平台验收情况统一见[发行状态](release-status.md)。安装器选择匹配宿主的 Android 15 x86_64 镜像，准备设备、应用、服务与画面，再进入模型配置。

缺少依赖时通过系统 PolicyKit 授权安装；不收集密码、不修改全局 Docker context、不授予 Docker 组权限。没有可用引擎时准备应用所属的 Lima/QEMU 容器宿主，Linux virtiofs 路径仍需实际验收。必须有硬件虚拟化及可访问的 `/dev/kvm`，不存在时明确阻止。

后台监督器用独占锁和真实进程启动身份保护已有任务；重复启动不替换运行进程，子进程崩溃只恢复所属服务。恢复已运行手机时只观察就绪，不重设网络或桌面。ready 要求真实手机 PNG/root 与电脑 PNG。数据目录与代码分离，停止不删除用户内容。

真实隔离 Linux 进程回归只覆盖相应生命周期合同，不能代替整条安装与恢复验收，也不能证明其他平台的手机桥接和双设备运行能力。各宿主的架构差异见[一体化宿主约定](integrated-hosts.md#platform-scope)。
