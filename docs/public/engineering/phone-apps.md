# Phone control applications

The optional `phone-apps` runtime component carries AI Phone's Unicode input/notification service, no-drawer launcher and notification authorization probe, plus a pinned guest-ABI-specific EinkBro browser. The input and launcher APKs are architecture-independent Java applications. Maintainers compile these resources with Android 35 tools; initial setup selects verified resources without Java, AAPT or D8.

## Build and ownership

```sh
. ./scripts/android-env.sh
./scripts/bun scripts/artifacts.ts phone --build
./scripts/bun scripts/build-client.ts --platform macos \
  --with-web-runtime --with-service-runtime --with-phone-apps \
  --android-runtime /path/to/android.payload
```

Factory builds run in an owned temporary tree. Dedicated development signing identities remain in ignored `.tools/phone-app-signing`, separate from installed device applications. Repeated builds reuse those identities; private keys never enter the component. These development signatures do not complete production signing/update review.

The build verifies each APK's signature and package ID, scans decompressed ZIP members for private material, then publishes a source-fingerprinted component to `.tools/phone-apps`. Changed sources or cache bytes fail explicitly; no existing component is overwritten. Changed inputs require a newly reviewed build cache. Factory failure cleans its temporary tree and does not touch devices.

Packaging binds APK sizes, SHA-256 hashes and signer metadata in `runtime/phone-apps/manifest.json`, inside the source payload's authenticated inventory. Staging rejects undeclared components. Runtime selection verifies both APKs, the probe and their matching source fingerprint. A present damaged component cannot fall back to compilation. Source-only contributor installs retain their compiler path.

The Android runtime and phone APKs are separate requirements: Java can be omitted only when both are supplied. An emulator-only capsule without the control applications stops before device creation. Startup installs only missing packages; existing input, launcher and application data retain their current device ownership.

## Input and notification readiness

The Rust host runtime owns the serial-bound input setup lock, installation intent and permission observations. Its private length-prefixed protobuf command is `--setup-phone-input`; bundled service manifests must qualify `ai-phone-input-setup-v2` before dispatch. Source contributors build the dedicated native helper; complete clients use the bundled assistant.

Setup verifies user 0, registered and selected Unicode input, hardware-keyboard input, notification access and the attention accessibility service. It repairs missing required capabilities while retaining unrelated input methods and accessibility services. Read-only observation never grants permissions or changes the selected keyboard. Read notification access from Android's notification service. Probe transfers are content-checked and omitted when the device already has matching bytes.

Existing `input-setup.pending.json` records remain readable. An installation intent is synced before the only install attempt; an unknown result is observed before recovery, and an absent package with an unresolved intent is not reinstalled. Existing APKs and application data are preserved. No model request, per-action reflection or database query participates in this bootstrap. These checks establish input/notification capabilities, not successful delivery of a real notification to an Agent or a complete host installation.

Offline setup installs the paged home screen and EinkBro browser. New phones show the browser, Settings and Files on the home screen. Apps installed later join the same screen. Input and notification services use the same serial lease. Existing apps, their data and the selected home are preserved.

EinkBro binaries, exact hashes, signer and source revision are pinned in [the browser policy](../../../config/phone-browser.json). Only the guest ABI is packaged. The new baseline uses Android’s system WebView. The previous development phone keeps its installed browser; the new baseline has not been installed there. Fresh-device setup installs a missing browser and preserves an existing one. New installers still require complete runtime and redistribution qualification.

## Availability

Current platform support, installer availability and distribution requirements are maintained in [release status](../release-status.md).

---

## 中文

手机控制组件包含 Unicode 输入/通知服务、无抽屉桌面及固定版本 EinkBro 浏览器的预构建 APK。构建阶段使用 Java 和 Android 工具，用户初始化时校验并安装随包 APK，不现场编译。独立开发签名保存在忽略的构建目录，不复用现有手机上的私有签名文件；完整生产签名与升级审查仍待完成。

输入初始化的锁、安装意图、输入法与通知授权核验由 Rust 负责。已安装输入应用也要核验必要能力，补齐时保留其他输入法和无障碍服务；只读核验不会改设置。通知权限以系统通知服务的实际返回为准，相同探针不重复传输。旧安装意图继续兼容，结果未知先观察，不直接重装。

首装在同一串行锁内安装分页桌面和 EinkBro 浏览器。新手机在桌面直接显示浏览器、设置和文件管理，后来安装的应用自动加入桌面。已有应用、数据及桌面选择继续保留。

组件同时绑定源指纹、包名、签名摘要和文件摘要，损坏时明确停止，不回退编译。只有模拟器和两个 APK 都齐备才跳过 Java；仅有模拟器会在创建设备前停止。启动流程继续只安装缺失应用，保留已安装应用与数据。

APK 交付、设备权限激活与完整初始化是独立环节。当前平台范围、签名与分发条件统一见[发行状态](../release-status.md)。
