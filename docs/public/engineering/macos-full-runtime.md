# Apple Silicon runtime distribution

The macOS DMG contains the client, Android and Linux baselines, local services and their runtimes. The client manages device creation.

The host requirement is an Apple Silicon Mac running macOS 15.0 or later, at least 12 GiB RAM and 35 GiB free space at initial deployment. The hardware and space checks run before device creation. Existing user data is kept separately from versioned program files.

## Contents

| Resource | Included implementation |
|---|---|
| Web | Next.js standalone application and Bun 1.4.4 |
| Agent and phone services | Host-native Rust assistant/runner and CPU OCR helper using managed Tesseract |
| Phone integration | Prebuilt input/notification and launcher applications |
| Container host | App-owned Lima 2.2.1, Alpine boot image and offline Docker/Compose packages |
| Media and connections | Compiled services and their locked production dependencies |
| Device networking | Clash Meta for Android and Clash Verge Rev for Linux |
| Android | AOSP Android 17 ARM64 (build 16644040), Avo 1.1.0 and ADB 36.0.1. [Sources and staging](android-runtime.md) |
| Linux and data services | Clean desktop/workspace images, PostgreSQL and Redis |

Resources are sealed by SHA-256 inventories. Runtime selection checks the host, component identity and file permissions. The distribution includes an inspectable source snapshot; chat, accounts, user files and device disks are never package inputs.

## Build

The complete factory requires an Apple Silicon build machine, an ARM64 Linux Docker build engine, Java 17 and the Android 35 application compiler SDK. These are maintainer tools; they are separate from the runtimes embedded for end users. Start from a fresh reviewed source checkout:

```sh
./scripts/bootstrap.sh
AIMANITY_ANDROID_SDK_INPUT=/factory/android-sdk \
./scripts/bun scripts/macos-client-factory.ts --compiler-sdk "$ANDROID_HOME"
```

The factory seals the staged AOSP, Avo and ADB files, builds the Lima container host, compiles the phone apps and creates the ARM64 desktop and service images. It writes `dist/macos-full-factory-report.json` after checking the DMG contents.

To reuse a sealed Android capsule, set `AIMANITY_ANDROID_PAYLOAD` and `AIMANITY_ANDROID_PAYLOAD_SHA256` in place of `AIMANITY_ANDROID_SDK_INPUT`. In the private **Desktop installer factory** workflow, select `platform=macos` and enter the Android capsule’s `android_draft_id` and `android_payload_sha256`. The ARM64 Linux job builds the image capsule, then the Mac job assembles the DMG.

For distribution, run the factory with `--sign-release` on a Mac with the Developer ID Application identity and the `notarytool` profile already in Keychain. Set `AIMANITY_MAC_SIGNING_IDENTITY` to the certificate name and `AIMANITY_MAC_NOTARY_PROFILE` to the profile name. The factory signs the app and DMG, submits the DMG to Apple, then staples and validates the ticket.

Private Actions storage retains the DMG, inventory and receipt. The maintainer publishes the reviewed installer to Releases as one asset under 2 GiB.

Use a reviewed source checkout and architecture-matched factory resources. The distribution command is:

```sh
./scripts/bun scripts/build-client.ts --platform macos \
  --with-web-runtime --with-service-runtime --with-phone-apps \
  --with-container-host  --with-guest-apps \
  --android-runtime /factory/android.payload \
  --images-runtime /factory/images.payload
```

Factory inputs must satisfy their source pins and redistribution requirements. The packager rejects compiled web routes absent from the public source selection. It compiles the application, checks its resources, and creates a verified compressed disk image containing the app and an Applications shortcut. Failed image creation preserves the previous artifact.

`server_included` describes the inspected service set. `runtime_payload_complete` and `complete_device_bundle` describe the matched host's complete resource inventory.

## Availability

The native ARM64 image factory compiles the Rust desktop bridge inside its pinned Linux build stage and runs a read-only version probe in the resulting desktop image. The dedicated-Mac software-engine route can instead supply a cross-compiled static Linux ARM executable through a source-bound image context. The application carries the resulting guest runtime and does not need an end-user compiler.

Precompiled guest inputs are verified and materialized inside the source checkout's shared factory directory before Docker is invoked. The image context contains only the executable and its source-bound manifest; external compiler caches are not engine mounts or package inputs.

[Release status](../release-status.md)

## 中文

macOS 发行结构内置客户端、Android 与 Linux 基础、服务程序及运行环境。终端用户无需另行安装编译器、包管理器、Android SDK 或 Docker。首次部署要求 Apple Silicon、macOS 15.0、12 GiB 内存和35 GiB可用空间；持久用户数据与程序版本分离。

内置 Android 17 ARM64（构建 16644040）、Avo 1.1.0 和 ADB 36.0.1。构建时将 `AIMANITY_ANDROID_SDK_INPUT` 指向暂存目录并运行上方命令。已有离线胶囊时，改用 `AIMANITY_ANDROID_PAYLOAD` 和 `AIMANITY_ANDROID_PAYLOAD_SHA256`。[下载](../release-status.md) · [Android 源码与暂存](android-runtime.md)
