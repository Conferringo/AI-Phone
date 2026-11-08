# Apple Silicon runtime distribution

The macOS DMG contains the client, Android and Linux baselines, local services and their runtimes. The client manages device creation.

The host requirement is an Apple Silicon Mac running macOS 14.4 or later, at least 12 GiB RAM and 35 GiB free space at initial deployment. The hardware and space checks run before device creation. Existing user data is kept separately from versioned program files.

## Contents

| Resource | Included implementation |
|---|---|
| Web | Next.js standalone application and Bun 1.4.4 |
| Agent and phone services | Host-native Rust assistant/runner and CPU OCR helper using managed Tesseract |
| Phone integration | Prebuilt input/notification and launcher applications |
| Container host | App-owned Lima 2.2.1, Alpine boot image and offline Docker/Compose packages |
| Media and connections | Compiled services and their locked production dependencies |
| Device networking | Android Clash applications and Clash Party in the Linux image |
| Android | ARM64 emulator, platform tools and Android 35 factory image |
| Linux and data services | Clean desktop/workspace images, PostgreSQL and Redis |

Resources are sealed by SHA-256 inventories. Runtime selection checks the host, component identity and file permissions. The distribution includes an inspectable source snapshot; chat, accounts, user files and device disks are never package inputs.

## Build

The complete factory requires an Apple Silicon build machine, an ARM64 Linux Docker build engine, Java 17 and the Android 35 application compiler SDK. These are maintainer tools; they are separate from the runtimes embedded for end users. Start from a fresh reviewed source checkout:

```sh
./scripts/bootstrap.sh
./scripts/bun scripts/macos-client-factory.ts --compiler-sdk "$ANDROID_HOME"
```

The factory collects the pinned ARM Android baseline, prepares a fresh matching emulator/ADB profile, builds the matched Lima runtime and offline container host, compiles the phone applications, and creates ARM64 desktop/workspace/database images. It passes all seven runtime components into the native packager and checks the resulting DMG inventory and SHA-256. The build report is written to `dist/macos-full-factory-report.json`. Complete assembly requires the selected Docker engine to be available on the build machine.

Run **Desktop installer factory** with `platform=macos` in the private build repository. A native ARM64 Linux job builds the image capsule. The Apple Silicon job verifies its source, architecture and inventory, assembles the DMG and retires the temporary handoff.

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

macOS 发行结构内置客户端、Android 与 Linux 基础、服务程序及运行环境。终端用户无需另行安装编译器、包管理器、Android SDK 或 Docker。首次部署要求 Apple Silicon、macOS 14.4、12 GiB 内存和35 GiB可用空间；持久用户数据与程序版本分离。

组件清单、首次双设备创建、升级与签名分别受发行条件约束。当前安装包供应与各平台可用范围统一见[发行状态](../release-status.md)。
