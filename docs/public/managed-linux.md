# Managed Linux host

The preview.3 source adds a client-owned Linux x64 installer for Ubuntu/Debian. **Fresh-host acceptance is in progress. This is a development bootstrap; it does not meet the complete bundled-distribution requirement. Product binaries have been withdrawn.** Linux ARM64 lacks a reviewed, architecture-matched Android SDK/emulator distribution and remains unsupported as an independent host.

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

Windows still needs a native phone/WSL backend adapter. A normal Android APK cannot use restricted AVF APIs to create arbitrary phone and GUI Linux guests. These are separate unfinished adapters, not capabilities inferred from Linux passing.

---

# Linux 客户端本机安装

preview.3 源码已接入 Ubuntu/Debian x64 本机安装链，仍在全新环境验收。此前产品下载已撤下；这条在线研发引导链不等于包含镜像、模拟器与预构建服务的完整发行包。安装器选择匹配宿主的 Android 15 x86_64 镜像，准备设备、应用、服务与画面，再进入模型配置。Linux ARM64 尚缺经审阅的匹配 SDK/模拟器分发，不标记为独立宿主可用。

缺少依赖时通过系统 PolicyKit 授权安装；不收集密码、不修改全局 Docker context、不授予 Docker 组权限。没有可用引擎时准备应用所属的 Lima/QEMU 容器宿主，Linux virtiofs 路径仍需实际验收。必须有硬件虚拟化及可访问的 `/dev/kvm`，不存在时明确阻止。

后台监督器用独占锁和真实进程启动身份保护已有任务；重复启动不替换运行进程，子进程崩溃只恢复所属服务。恢复已运行手机时只观察就绪，不重设网络或桌面。ready 要求真实手机 PNG/root 与电脑 PNG。数据目录与代码分离，停止不删除用户内容。

真实隔离 Linux 进程回归已通过；干净 GitHub runner 的整条安装及恢复验证进行中。Windows、Android 独立适配仍未完成，不因 Linux 代码存在或安装包编译通过而宣称四平台完成。
