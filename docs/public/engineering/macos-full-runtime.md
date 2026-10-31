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

## Qualification

The 2026-10-31 local build produced a 2.82 GiB application and a 2.70 GiB disk image. All 1,675 application files were compared with the read-only mounted image. Its source/runtime archive contains 9,557 files; portable installer tools contain 1,660 files.

The mounted distribution supplied Python, web, native services and OCR for an isolated execution check. The operating system denied WAN access, the original checkout and the host SDK; denial probes were checked explicitly. The web page, thirteen script assets, PNG output, native version commands, OCR and JavaScript component identities passed without package installation or compilation. The packaged assistant also applied all thirty migrations through schema version 32 to a disposable PostgreSQL instance; the runtime role was unprivileged and could not perform DDL.

The compiled application opened the workbench against the retained local installation. This verifies the native entry and rendering path. A fresh default-size Android/Linux deployment, installation upgrades, Developer ID signing/notarization and full platform/UI/model/channel acceptance remain separate qualification items. See [release status](../release-status.md).

## 中文

macOS 发行结构内置客户端、Android 与 Linux 基础、服务程序及运行环境。终端用户无需另行安装编译器、包管理器、Android SDK 或 Docker。首次部署要求 Apple Silicon、macOS 14.4、12 GiB 内存和35 GiB可用空间；持久用户数据与程序版本分离。

本地完整 App 和磁盘映像已编译，并核对映像中全部1,675个文件。系统隔离下，使用包内 Python 实际启动网页、原生服务/OCR并校验JS组件；外网、本机源码与SDK被拒绝访问。随包助手已在临时数据库应用30项迁移（最新版本32），运行角色无DDL权限。全新默认双设备创建、升级、签名及全部平台/模型/渠道/界面验收仍按发行门槛逐项确认。
