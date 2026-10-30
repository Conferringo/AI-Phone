# Phone control applications

The optional `phone-apps` runtime component carries AI Phone's Unicode input/notification service and no-drawer launcher. Both APKs are architecture-independent Java applications. Maintainers compile them with Android 35 tools; initial setup selects verified APKs without Java, AAPT or D8.

## Build and ownership

```sh
. ./scripts/android-env.sh
python3 scripts/prebuilt_phone.py --build
python3 scripts/build_client.py --platform macos \
  --with-web-runtime --with-service-runtime --with-phone-apps \
  --android-runtime /path/to/android.payload
```

Factory builds run in an owned temporary tree. Dedicated development signing identities remain in ignored `.tools/phone-app-signing`, separate from installed device applications. Repeated builds reuse those identities; private keys never enter the component. These development signatures do not complete production signing/update review.

The build verifies each APK's signature and package ID, scans decompressed ZIP members for private material, then publishes a source-fingerprinted component to `.tools/phone-apps`. Changed sources or cache bytes fail explicitly; no existing component is overwritten. Changed inputs require a newly reviewed build cache. Factory failure cleans its temporary tree and does not touch devices.

Packaging binds APK sizes, SHA-256 hashes and signer metadata in `runtime/phone-apps/manifest.json`, inside the source payload's authenticated inventory. Staging rejects undeclared components. Runtime selection verifies the complete two-APK inventory and matching source fingerprint. A present damaged component cannot fall back to compilation. Source-only contributor installs retain their compiler path.

The Android runtime and phone APKs are separate requirements: Java can be omitted only when both are supplied. An emulator-only capsule without the control applications stops before device creation. Startup installs only missing packages; existing input, launcher and application data retain their current device ownership.

## Validation scope

Both actual APKs have been compiled, signature-verified and checked for expected package identities. The component has been packaged, expanded into an isolated installation and selected by the included Python while OS network access is denied and PATH contains no Java/SDK compiler. An isolated chat fixture remains unchanged. This verifies artifact delivery and compiler independence; fresh Android installation and permission activation were not exercised in this batch.

Tests cover source changes, changed files, identities, inventory, compressed private data, bounded headers, retained signing keys, failed compilers and staging. The full client still requires its container host, media/channel components, redistribution review and platform first-run acceptance. See [release status](../release-status.md).

---

## 中文

手机控制组件包含 Unicode 输入/通知服务与无抽屉桌面的预构建 APK。构建阶段使用 Java 和 Android 工具，用户初始化时校验并安装随包 APK，不现场编译。独立开发签名保存在忽略的构建目录，不复用现有手机上的私有签名文件；完整生产签名与升级审查仍待完成。

组件同时绑定源指纹、包名、签名摘要和文件摘要，损坏时明确停止，不回退编译。只有模拟器和两个 APK 都齐备才跳过 Java；仅有模拟器会在创建设备前停止。启动流程继续只安装缺失应用，保留已安装应用与数据。

本批真实编译/签名校验、系统禁网环境下的封装、展开与无编译器选择通过，隔离聊天夹具保持。未在新 Android 上安装这两个 APK 或启用权限，不将组件检查写成整机验收。完整容器宿主、媒体/渠道、平台初始化与再分发审查继续推进。
