# Multi-platform client evidence / 多平台客户端验收

Current status (2026-10-30): source is public; complete installers are not available. Native CI has compiled macOS ARM64, Windows x64, Linux x64/ARM64 and Android ARM64. Matching [web-component checks](runtime-bundles.md) now verify relocated desktop runtime bytes. This does not prove independent two-device hosting. Android hosting targets root/KVM devices. Non-macOS physical installation is waived and remains unverified. See [release status](release-status.md) and [current Actions](https://github.com/Conferringo/AI-Phone/actions).

## Historical runtime checks — 2026-10-28

The following records preserve the original preview's failures and runtime evidence. Those downloads were withdrawn; their tests are not complete-product acceptance. Later notices and host decisions are owned by the documents above.

2026-10-28 · `0.1.0-preview.1` · Local previews only / 仅本地预览

The client and server are separate deliverables. These installers open an existing local/LAN deployment; they contain no guest disks, server runtime, API keys, accounts, conversations or signing keys. Four client artifacts do **not** mean four complete server installers.

客户端与服务器分开交付。当前安装包连接已部署服务，不带设备系统盘或用户数据；全新主机整套免编译安装仍是独立门槛。

| Target / 目标 | Artifact / 制品 | Evidence / 证据 | Pending / 未验证 |
| --- | --- | --- | --- |
| macOS ARM64 | DMG / app | Actual native UI, connection recovery/switching, media, Chinese input; inspected resources and DMG checksum | Developer ID, notarization, other supported macOS versions, clean host |
| Windows x64 | NSIS EXE | Actual MSVC cross-build, expanded NSIS payload, x64 PE, legal/private-material checks | Windows installation, WebView2 bootstrap, input/media, uninstall/upgrade, Authenticode |
| Linux ARM64 | DEB | Debian13 package install, non-root startup, native connection error, saved-address startup and live phone/computer frames; optimized GLib regressions | Independent physical host, clipboard/IME, audio/files, Ubuntu/Fedora, Wayland |
| Android ARM64 | APK | Android15 installation and launch, native address editing, LAN workbench/live frames, system Back to connection | Physical phones, Android9–14/16, long-duration stream stability, keyboard/insets/rotation matrix, download/share, production signature |

Windows ARM64, Linux x64 binaries, macOS Intel, AppImage and RPM are not present in this local delivery. CI includes Linux x64 build preparation, but an unrun workflow is not a successful build.

## Fixes backed by failures / 失败驱动的修复

- Android release builds initially crashed in Jackson enum reflection because the regenerated AGP8 build omitted the standard Android ProGuard rules. The project now applies `proguard-android-optimize.txt`; R8 remains enabled. The installed release then opened successfully. AndroidX, pinned SDK/Gradle/AGP versions and branded icons are reproducible overlays, not manual edits to an ignored folder.
- Mobile uses one Activity/WebView. Native Back returns from the workbench to the local connection page, and then backgrounds the application. Remote pages gain no native IPC permissions. Desktop menus remain desktop-only.
- Linux's GLib advisory is explicitly tracked with the exact upstream two-line fix, a complete vendored source hash manifest, two release-mode iterator regressions and tamper tests. See [backport provenance](native-backports.md). Path dependencies omitted by cargo-audit are explicitly restored to the report with their verified mitigation.
- Initial desktop window dimensions are bounded by monitor work area. The Linux test display exposed a 1440×980 window exceeding a 1280×800 screen. Size selection now leaves room for desktop chrome.
- Rust's native library has a distinct name from the Windows executable to avoid colliding debug/import output names. Windows cross-build caches must use paths without whitespace; source directories may contain spaces.
- Every native payload is expanded before inspection. Legal files must match the sources, known credential patterns and developer home paths are rejected, and architecture/format are checked. APK signatures and DMG checksums are independently verified. This is a packaging guard, not a claim of comprehensive security certification.

## Reproduction / 复现

Use Python3.11+, Node22 and Rust1.96.0. Install the pinned client CLI with `python3 scripts/bootstrap_bun.py --dependencies`, or use the project Bun bootstrap. Then run:

```sh
python3 scripts/build_client.py --platform macos
python3 scripts/build_client.py --platform windows
python3 scripts/build_client.py --platform linux
python3 scripts/build_client.py --platform android
```

Choose the appropriate command/toolchain, not all four on an unsupported host. Linux builds natively on Linux. Windows uses MSVC natively or cargo-xwin0.22.0 + LLVM/lld-link + NSIS for cross-build; 7-Zip inspects NSIS contents. Android requires SDK36, build-tools36.0.0, NDK27.0.12077973 and JDK17; regenerated Gradle8.14.3/AGP8.11.1 scaffolding is ignored. Android minimum API28 is a build declaration, not evidence that all API28 devices passed acceptance.

The DEB declares glibc2.39+, GTK3.24+ and WebKitGTK2.40+ prerequisites. The inspected ARM64 binary's highest GLIBC symbol requirement is2.39. Actual GUI testing used an isolated Debian13/X11 container as a non-root user, viewed through a loopback-only VNC transport. Initial server failure was clicked in the GUI. The successful Linux startup used a seeded test-only connection file; VNC modifier/clipboard issues mean manual address replacement and IME are **not** marked accepted. Android GUI actions used the existing workbench's real device control; only this project's test APK was installed.

## Distribution boundary / 发行边界

At the dated review, artifacts remained local. Windows was unsigned; macOS lacks Developer ID signing/notarization; Android uses an isolated local preview certificate. The Android key/password remain in ignored `data/client-signing` and must never be uploaded. A production signing identity and upgrade migration from preview builds require separate preparation. SHA256 is integrity evidence, not publisher identity.

Each artifact has a `.sha256` and `.manifest.json` containing architecture, signing/runtime limits, source-lock hashes and unpacked file hashes. Do not distribute the whole `dist` directory: it may also hold private local evidence or review trees. Release only individually approved artifacts after their gates pass.

LAN HTTP also exposed direct `crypto.randomUUID()` calls in desktop controls, transfers, provisioning and registry creation. These now use the existing cryptographic fallback, with 1,000 unique action IDs covered by regression checks.

Native CI tests and builds macOS, Windows, Linux and Android clients. The [Windows backend factory](windows-backend-runtime.md) separately assembles its Linux rootfs and qualifies the owned backend runtime. Installer publication requires complete platform resources, privacy and redistribution checks, installation scope, long-running recovery, UI/media acceptance and update activation/rollback. Current platform evidence is maintained in [release status](release-status.md).

Android media note: one repeated-start test showed the desktop stream stalled while the phone stream remained live. Re-entering the workbench restored both streams. Diagnostic codes were added without frame/user content, but the original stall is not yet attributed to a proven cause; long-duration acceptance remains open. / Android复验曾出现仅电脑画面断流，重新进入后恢复；已加无内容诊断，但根因未充分证实，不把它写成已修复。
