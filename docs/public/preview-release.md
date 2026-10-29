# Preview clients / 预览客户端

[Downloads / 下载](https://github.com/Conferringo/AI-Phone/releases/tag/v0.1.0-preview.2) · [Issues](https://github.com/Conferringo/AI-Phone/issues/new/choose)

Preview.2 adds experimental **managed local setup on macOS Apple Silicon**. The client acquires tools and builds services/devices from its verified bundled source; users then configure a model in the workbench. Fresh-host dual-device installation remains untested. Windows/Linux/Android remain connection clients. Guest images, toolchains, Docker images, accounts and user files are not embedded in the installer. See [client-managed deployment](integrated-hosts.md).

macOS Apple Silicon 已加入实验性本机安装，不再只提供连接入口。工具与客体在安装时下载/构建；全新宿主安装仍未实测。Windows/Linux/Android 安装包继续连接已有服务，独立宿主适配未完成。

| Package | Target | Build/inspection | Installation/runtime |
|---|---|---|---|
| macOS DMG | Apple Silicon; macOS 14.4+ | Native CI build; expanded resources/privacy checked | Native setup/recovery and real existing-service/frame checks; no clean-host installation test. No Developer ID signing/notarization |
| Windows EXE | x64; WebView2 required | Native Windows CI build; NSIS payload/PE inspected | Not installed or runtime-tested; unsigned, SmartScreen warnings possible |
| Linux DEB | amd64 and arm64; built on Ubuntu 24.04 | Native CI build; expanded DEB/architecture inspected | Not installed or desktop/media-tested; other distributions unverified |
| Android APK | arm64-v8a; Android 9+ | Release build; native ELF/legal payload inspected; signature checked | Not installed or touch/background-tested; private preview certificate, not a store identity |

实机安装验收由维护者决定暂不执行；未实测不标为通过。请反馈系统版本、CPU架构、客户端版本、复现步骤及脱敏日志；不要上传密钥、订阅、账号、联系人、聊天截图或备份。安全漏洞用仓库私密报告入口。

Unsigned desktop builds and the Android preview identity are disclosed limitations. Do not disable system-wide security protections. Android updates require the same signing identity; the published APK is signed by the maintainer's retained private preview key, not the ephemeral CI identity. The server/device/chat state is separate from the client bundle.

## Integrity and contents

Each installer has a SHA256 file and an inspection manifest. The manifest records the source commit, locked dependencies, declared notice hashes and runtime/signing limits. Compare checksums before installing. Checksums are integrity evidence, not a substitute for code signing.

The release also includes source and a third-party notice archive. Rust upstream grants/full terms, Android POM/NOTICE attribution and MPL source URLs are preserved. See [third-party notices](../../THIRD_PARTY_NOTICES.md). This does not grant rights to third-party services, trademarks or SDKs beyond their own terms.

## Publishing process

1. Source/native CI checks compile and inspect the same commit. They do not operate social accounts, paid models or real user devices.
2. Artifact upload contains only inspected installers, SHA256 files and manifests; signing keys and user state stay private.
3. The maintainer downloads the artifacts, verifies the source commit/payload/notice hashes, signs the Android APK with the retained preview identity and prepares a draft Release.
4. The draft is published as a prerelease after asset checks. No stable/full-product acceptance is implied; open gates remain in `config/release-policy.json`.

For subsequent versions, never replace a published tag or asset silently. Use a new preview version. Preserve Android signing identity; desktop signing/notarization and signed automatic updates remain future work.
