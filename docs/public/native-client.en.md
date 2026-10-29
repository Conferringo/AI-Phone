# Native client preview

[简体中文](native-client.zh-CN.md) · 2026-10-28 · Local macOS ARM64 test package. Declared minimum: 14.4; actual validation host: 26.5.1. Other OS versions still need physical-machine acceptance.

A real Tauri 2 client, application bundle and DMG now exist. Launch opens the configured workbench directly, without a welcome/forest/Enter screen. Personal mode restores the owner session and retains accounts/conversations. Configure devices, models and APIs in the existing single configuration center. Closing the client does not stop background Agents or device services.

This package requires an **already deployed, running server**. It does not contain the server stack, Android SDK, guest disks, Docker, user state or keys. A compiler-free clean-host installer, managed runtime acquisition and install/update orchestration remain unfinished. Additional platform build/runtime evidence is listed below.

## Use

1. Complete [source deployment](getting-started.en.md) and configure devices/Agents first.
2. Open the DMG, drag AI Phone.app to Applications and launch it. The existing local deployment defaults to `http://127.0.0.1:17860/phone`.
3. To use your own LAN server, select AI Phone → Connection… (Command+,), enter its root address and connect. Only a successful address is saved. Failures can be corrected/retried. Only localhost/private IPs are accepted; embedded credentials, query strings and page paths are rejected.

Native configuration stores only the server address, not model keys, accounts or chat, at `~/Library/Application Support/dev.aiphone.client/server.json` on macOS. The workbench receives no arbitrary native execution capability. Only the local connection window can use the two configuration commands; backend window/origin checks also apply. HTTP is not encryption; trusted pairing/TLS across untrusted networks remains a release requirement.

`./ai-phone open` prefers a built/installed client with this project's Bundle ID and otherwise opens the browser workbench. It does not overwrite the client's saved server address.

## Build and verify

```sh
./scripts/bootstrap.sh
cargo test --manifest-path apps/client/src-tauri/Cargo.toml --locked
python3 scripts/build_client.py
```

The pinned CLI/Rust graph produces `dist/AI-Phone-<version>-macos-arm64.dmg`, SHA256 and a manifest. The client has its own Cargo workspace so native Linux WebKit dependencies do not silently enter the existing server CI; a dedicated macOS workflow now prepares native tests, Clippy, advisory checks and unsigned packaging. It has not run remotely. Windows/Linux/Android workflows are also prepared; remote CI and complete native-machine acceptance remain pending.

The local preview has no Developer ID signing/notarization, only the Mach-O linker's ad-hoc signature. It is not a store/global-release installer. Native connection UI has Chinese/English labels; the workbench reuses eight catalogs. Complete language, WebKit media/input and attachment acceptance is tracked separately.

Bundled resources include project licences, privacy/disclaimer and locked client dependency notice texts. Some upstream crate archives omit complete licence files; those gaps remain recorded. Final binary/image redistribution review has not passed. See [third-party notices](../../THIRD_PARTY_NOTICES.md) and [release status](release-status.md). No source, package or private state is uploaded.

The build inspects every file in the macOS bundle: identity/version/minimum OS, legal-resource equality, unexpected private files, developer home paths and credential patterns. The DMG checksum and the bundle file hashes are recorded. See the [client release audit](client-release-audit-2026-10-28.md); a checksum is not a signing/notarization substitute.


## Additional platform previews — 2026-10-28

Local packages now exist for Windows x64 (NSIS), Linux ARM64 (DEB) and Android ARM64 (APK). See the [platform evidence and limits](multiplatform-clients.md). Windows is cross-compiled and unpacked/inspected, **not runtime-tested on Windows**. Linux was installed and launched as a non-root user in Debian 13; Android was installed and exercised on the existing Android 15 instance. Physical-device acceptance remains separate.

Use `python3 scripts/build_client.py --platform windows|linux|android` with one platform value. Linux builds on Linux and requires glibc 2.39+, GTK3.24+ and WebKitGTK2.40+. Windows requires MSVC or cargo-xwin/LLVM lld-link/NSIS, plus 7-Zip for payload inspection; the cross-build SDK cache must have no whitespace in its path. Android requires JDK17, SDK/build-tools36, NDK27.0.12077973 and the Rust ARM64 Android target. Versioned overlays regenerate the ignored Android project, including standard ProGuard reflection/JNI rules and project icons.

On Android, enter the server's configured private LAN address. Back returns to the native connection screen; Back again backgrounds the app. The APK is signed with an isolated local preview identity, not a production/distribution key. Private signing state is ignored under `data/client-signing`. Windows packages are unsigned; macOS is not Developer ID signed/notarized. All packages connect to an existing deployment and include no server/device images, credentials or user files.
