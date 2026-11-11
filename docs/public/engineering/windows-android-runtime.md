# Windows Android runtime

The Windows x64 component packages the native emulator, ADB and Android API 35 x86_64 image. The headless profile includes the x86_64 QEMU engine, launcher, GPU libraries, disk tools and upstream notices.

The [factory policy](../../../config/android-factory.json) pins the stable channel, SDK license identifier and archive digests. The [producer](../../../scripts/android-component.ts) checks extracted revisions and generates the component inventory. The [Windows workflow](../../../.github/workflows/windows-android-component.yml) checks the relocated emulator and ADB version commands and retains the JSON receipt.

## Build

Run on Windows x64 with the project Bun and Rust toolchain. Use a new work directory.

```sh
./scripts/bun scripts/android-component.ts --work .tools/windows-android-component --verify-programs --receipt dist/windows-android-component.json
```

The [Windows installer factory](../windows-client-factory.md) pairs this Android component with the QEMU Linux guest. The Rust bootstrap provisions the phone and waits for the paired device receipt. [Platform and distribution status](../release-status.md)
