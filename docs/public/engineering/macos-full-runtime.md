# Apple Silicon runtime distribution

AI Phone's macOS distribution contains the client, Android and Linux baselines, service binaries and their execution runtimes. Device creation is managed by the client. End users do not need to install a compiler, package manager, Android SDK or Docker separately.

The host requirement is an Apple Silicon Mac running macOS 14.4 or later, at least 12 GiB RAM and 35 GiB free space at initial deployment. The hardware and space checks run before device creation. Existing user data is kept separately from versioned program files.

## Contents

| Resource | Included implementation |
|---|---|
| Web | Next.js standalone application and Bun 1.4.4 |
| Agent and phone services | Host-native Rust assistant/runner and Apple Vision OCR |
| Phone integration | Prebuilt input/notification and launcher applications |
| Container host | App-owned Lima 2.2.1, Alpine boot image and offline Docker/Compose packages |
| Media and connections | Compiled services and their locked production dependencies |
| Device networking | Android Clash applications and Clash Party in the Linux image |
| Android | ARM64 emulator, platform tools and Android 35 factory image |
| Linux and data services | Clean desktop/workspace images, PostgreSQL and Redis |
| Installer execution | Python 3.13.18 and Node.js 22.23.4 |

Resources are sealed by SHA-256 inventories. Runtime selection checks the host, component identity and file permissions. The distribution includes an inspectable source snapshot; chat, accounts, user files and device disks are never package inputs.

## Build

Use a reviewed source checkout and architecture-matched factory resources. The distribution command is:

```sh
python3 scripts/build_client.py --platform macos \
  --with-web-runtime --with-service-runtime --with-phone-apps \
  --with-container-host --with-js-services --with-guest-apps \
  --android-runtime /factory/android.payload \
  --images-runtime /factory/images.payload \
  --installer-tools /factory/tools
```

Factory inputs must satisfy their source pins and redistribution requirements. The packager rejects compiled web routes absent from the public source selection. It compiles the application, checks its resources, and creates a verified compressed disk image containing the app and an Applications shortcut. Failed image creation preserves the previous artifact.

`server_included` describes the inspected service set. `runtime_payload_complete` and `complete_device_bundle` describe the matched host's complete resource inventory. These fields are independent of installation acceptance, signing and binary-publication gates.

## Availability

Current platform support, installer availability and distribution requirements are maintained in [release status](../release-status.md). Component formats and build interfaces do not independently qualify a complete installation.

## 中文

macOS 发行结构内置客户端、Android 与 Linux 基础、服务程序及运行环境。终端用户无需另行安装编译器、包管理器、Android SDK 或 Docker。首次部署要求 Apple Silicon、macOS 14.4、12 GiB 内存和35 GiB可用空间；持久用户数据与程序版本分离。

组件清单、首次双设备创建、升级与签名分别受发行条件约束。当前安装包供应与各平台可用范围统一见[发行状态](../release-status.md)。
