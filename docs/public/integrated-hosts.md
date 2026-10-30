# Developer bootstrap and host runtime

**Development reference only. Product downloads are withdrawn. The next distribution must contain images, emulator and prebuilt services; see [requirements](preview-release.md). The online source bootstrap below is not that distribution.**

Preview.2 introduces an experimental managed installer for **macOS Apple Silicon**. It acquires private tools, prepares the Android phone and Debian/Openbox desktop, starts services and then opens model configuration. Users do not need to run server deployment commands for this path. It still downloads dependencies and builds services; it is not an offline appliance. **Fresh-host dual-device installation remains untested.**

## Platform scope

| Host | Implemented in this preview | Remaining |
|---|---|---|
| macOS Apple Silicon | Native setup, resource presets, pinned tool acquisition, Android licenses, resumable phases, owned service startup and desktop provisioning | Complete clean-host installation, reboot/long-running acceptance, signed updates |
| Windows x64 | Local/LAN connection client | WHPX, managed WSL2 backend, architecture-matched phone image, native supervision/vision |
| Linux amd64/ARM64 | Local/LAN connection client | KVM permissions, distribution-aware dependencies, user services and vision backend |
| Android ARM64 | Local/LAN connection client | Privileged AVF/KVM research and device matrix; ordinary unrooted APK dual-guest hosting is not promised |

The Linux desktop is a container, sharing the container host's kernel. A separate-kernel Fedora/QEMU desktop is an extension, not a current choice. This preview uses fixed Android 15 ARM64 and Debian 13/Openbox templates. CPU, memory and initial phone storage are configurable; changing a preset never silently resizes existing devices.

## Install and recovery

1. Open the client, choose a language and phone/computer resources, then install. A fresh install requires 12 GiB RAM minimum and 35 GiB free storage; 16 GiB RAM or more is recommended.
2. The client downloads pinned tools into its own directory. Complete macOS developer-tool installation if requested, and review each Android license yourself. It does not collect a sudo password or automatically accept terms.
3. The installer verifies downloads, source inputs and completed artifacts. Android, service dependencies, guest applications and desktop images have separate resumable checkpoints.
4. After services and a real desktop frame are verified, choose Configure model. Model configuration, keys and bindings live in the workbench's single configuration center.
5. Subsequent launches open a reachable deployment directly. If this exact client payload has a completed install checkpoint, it can resume the owned deployment and probe it before opening. Incomplete installation offers Retry; a saved checkpoint never proves that services are currently running.

Closing during installation stops its child process group and retains completed work. Stop also interrupts a network peer that stalls before response headers or body data. A phase-completion event does not mark the whole installation ready; the installer process must exit successfully. An SDK license pending at interruption is presented again, never automatically accepted from a saved prompt.

Already running services are left intact. A registered but stopped owned launchd job uses kickstart without forced termination. Foreign jobs/containers and quarantined restored data are rejected before startup changes. This is not full installation rollback or an automatic updater.

## Data ownership

`host/releases/<payload hash>` contains immutable application source. `host/user/data` and `host/user/.env` retain devices, database state and configuration separately. Host tools and caches are private to the app. The remote workbench has no native installer IPC authority. An existing deployment may also be connected without adoption or copying its state.

A healthy existing Docker engine can be reused; otherwise the macOS adapter prepares an app-owned Lima/VZ container host with a scoped data mount. It does not change the global Docker context. Guest Clash applications are installed inside the devices; no proxy subscriptions are distributed. [Lima VZ](https://lima-vm.io/docs/config/vmtype/vz/), [Android acceleration](https://developer.android.com/studio/run/emulator-acceleration).

## Evidence and open gates

Local native tests exercise actual child processes, durable checkpoints, failed-ready rejection, cancellation of stalled loopback HTTP and resource validation. Isolated Python tests cover source staging, user-data separation, retries, license prompts and service ownership. A real existing installation was resumed without changing any of its seven process IDs; its authenticated desktop observation returned a real frame. These checks do not constitute a clean-host installation or a physical-platform test.

Windows/Linux managed hosts, Android privileged hosting, full install/upgrade rollback, signed updates and whole-system long-running recovery remain open in [release status](release-status.md).

---

# 研发引导与宿主运行时

Preview.2 为 macOS Apple Silicon 提供实验性受管安装：在客户端选语言与设备资源，准备工具、Android、服务和 Linux 桌面，再进入模型配置，不必手动部署服务。全新宿主创建双设备仍未实测。Windows、Linux、Android 安装包继续用于连接已有服务，没有冒充四个平台均已支持独立运行。

固定模板为 Android 15 ARM64 与 Debian 13/Openbox；初始 CPU、内存和手机存储可选，已有设备不会随预设变化被重置或偷偷扩容。首次需要至少 12 GiB RAM、35 GiB 可用空间，建议 16 GiB RAM。开发工具或 Android 许可需要用户完成确认。

安装进度与资源分开持久化。阶段完成不等于安装完成；失败进程的 ready 事件不授权自动恢复。重开后，中断安装提供重试；已装完的相同源码版本先恢复所属服务并真实探测，再打开工作台。停止可中断下载和子进程组。正在执行的任务不重启，已停的受管进程可无强制终止地启动。用户设备、账号、聊天、文件和密钥不进入源码包；代码版本与用户数据分离。

真实现有环境已验证七个服务进程 ID 不变、协议接口和电脑画面可用。这不等于全新安装、整机重启或四平台独立宿主验收；未完成项保持公开。
