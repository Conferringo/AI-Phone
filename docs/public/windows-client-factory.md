# Windows installer factory

The Windows 11 x64 EXE contains the Tauri client, the native Android emulator and image, and a QEMU Linux guest. Windows Hypervisor Platform accelerates both devices. The client uses system WebView2 and links its C runtime statically.

## Build

Prepare the [QEMU host and Linux guest](windows-backend-runtime.md#qemu-build-inputs) from the same source revision. Assemble on Windows x64 with the pinned Bun and Rust toolchains.

```sh
./scripts/bun scripts/windows-client-factory.ts build-qemu \
  --qemu build/windows-qemu --qemu-identity "$QEMU_MANIFEST_SHA256" \
  --guest build/windows-guest.payload --guest-identity "$GUEST_SHA256"
```

In a private build repository, run **Desktop installer factory** with `platform=windows`. The workflow builds QEMU and the sealed Linux guest in parallel, assembles the EXE with Inno Setup and stores the installer and receipts as private artifacts.

The inspector checks the x64 executable and installed-file inventory. Setup enables Windows Hypervisor Platform and initializes a persistent QCOW2 overlay over the packaged Linux disk. The Android disk and application data have separate storage. [Build and installation status](release-status.md)

## 中文

Windows 11 x64 安装包包含 Tauri 客户端、Android 模拟器与镜像、QEMU Linux 客体。Windows Hypervisor Platform 加速两台设备，客户端使用系统 WebView2。应用数据与程序资源分开存放。

在私有构建仓库运行 **Desktop installer factory** 并选择 Windows。工作流并行构建 QEMU 和 Linux 客体，再用 Inno Setup 生成并检查单个 EXE。[构建与安装状态](release-status.md)
