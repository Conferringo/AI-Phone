# Windows installer factory

The Windows 11 x64 EXE contains the Tauri client, AOSP Android 15 x64 (build 13455678) and a Linux guest. Bundled QEMU 11.1.3 runs both guests through Windows Hypervisor Platform. The client uses system WebView2 and links its C runtime statically. [Android sources and staging](engineering/android-runtime.md)

## Build

Prepare the [QEMU host and Linux guest](windows-backend-runtime.md#qemu-build-inputs) from the same source revision. Assemble on Windows x64 with the pinned Bun and Rust toolchains.

```sh
AIMANITY_ANDROID_SDK_INPUT=/factory/android-sdk \
./scripts/bun scripts/windows-client-factory.ts build-qemu \
  --qemu build/windows-qemu --qemu-identity "$QEMU_MANIFEST_SHA256" \
  --guest build/windows-guest.payload --guest-identity "$GUEST_SHA256"
```

To reuse a sealed Android capsule, set `AIMANITY_ANDROID_PAYLOAD` and `AIMANITY_ANDROID_PAYLOAD_SHA256` in place of `AIMANITY_ANDROID_SDK_INPUT`. In the private **Desktop installer factory** workflow, select `platform=windows` and enter the Android capsule’s `android_draft_id` and `android_payload_sha256`. The workflow builds QEMU and the Linux guest in parallel, then assembles the EXE with Inno Setup.

For distribution, add `--sign-release` on a Windows builder with SignTool and a trusted code-signing certificate in the current user’s certificate store. Set `AIMANITY_WINDOWS_CERT_THUMBPRINT` to the certificate thumbprint and `AIMANITY_WINDOWS_TIMESTAMP_URL` to its RFC 3161 timestamp service. The factory signs, timestamps and verifies both the client and the installer.

The inspector checks the x64 executable and installed-file inventory. Setup enables Windows Hypervisor Platform and initializes a persistent QCOW2 overlay over the packaged Linux disk. The Android disk and application data have separate storage. [Build and installation status](release-status.md)

## 中文

Windows 11 x64 安装包内置 Tauri 客户端、AOSP Android 15 x64（构建 13455678）和 Linux 客体。两台设备通过 QEMU 11.1.3 和 Windows Hypervisor Platform 启动，客户端使用系统 WebView2。

在私有构建仓库运行 **Desktop installer factory**，选择 Windows，填写 Android 胶囊的 `android_draft_id` 和 `android_payload_sha256`。本地构建时，将 `AIMANITY_ANDROID_SDK_INPUT` 指向暂存目录并运行上方命令。[下载](release-status.md) · [Android 源码与暂存](engineering/android-runtime.md)
