# Windows Android runtime component

The Windows resource factory builds the x64 emulator, platform tools and Android API 35 x86_64 image as one sealed component. It supplies the native Android resources for independent Windows hosting. WHPX/WSL integration, initialization and recovery have separate qualification.

New Windows capsules retain the x86_64 headless QEMU engine, launcher, shared
runtime/GPU libraries, ADB, disk tools and upstream notices. Other QEMU engines and
the standalone Qt UI are omitted from the new inventory. Selection belongs to the
Rust maintainer tool and runs before file hashing; it does not delete files from
the SDK input or edit an existing installation. The packaged version probe uses
`-no-window`. Native Windows execution and a complete EXE build must qualify this
profile; PE import inspection alone cannot prove transitive/dynamic dependencies
or guest boot.

The [factory policy](../../../config/android-factory.json) shares the guest image pin with Linux while keeping native SDK archives separate. Stable SDK selection requires Google's stable channel and license identifier. Archive size, vendor SHA-1 and independently computed SHA-256 are recorded. The [component producer](../../../scripts/android-component.ts) checks extracted SDK revisions against those pins before packaging. No existing AVD, account or guest disk is an input.

Admission uses `.exe` program names, canonical Windows filenames, case-insensitive collision checks and directory-junction rejection. Windows file permissions use the read-only attribute and the installer’s private-directory boundary. Archive-body hashes remain mandatory on every platform. Unsupported host manifests and incomplete components cannot promote a client to independent deployment.

The [Windows workflow](../../../.github/workflows/windows-android-component.yml) builds from a fresh checkout, relocates the component, verifies its inventory and invokes the packaged emulator and ADB version commands under an isolated profile. Its receipt explicitly separates those checks from guest boot and WHPX/WSL lifecycle qualification. Only JSON receipts are retained; SDK and system-image binaries remain on the disposable worker while redistribution review is pending.

## Maintainer build

Run on Windows x64 with the project Bun and Cargo toolchain. The work directory must be new. Device images are large; keep the component’s recovery reserve available.

```sh
./scripts/bun scripts/android-component.ts --work .tools/windows-android-component --verify-programs --receipt dist/windows-android-component.json
```

Full Windows delivery still requires the application-owned Linux/backend environment, Windows acceleration setup, device-control bridge, initialization and recovery. A successful SDK receipt alone does not qualify that delivery.

The [Rust native preparation path](native-android-preparation.md) now admits this offline component and creates or recovers the device configuration. Windows client builds accept `--android-runtime` alongside `--wsl-runtime`; the native bootstrap uses the same installation lock and leaves complete-host readiness disabled. This resource integration is separate from launching and observing a real Windows Android guest.

## Availability

Current platform support, installer availability and distribution requirements are maintained in [release status](../release-status.md). Component formats and build interfaces do not independently qualify a complete installation.
