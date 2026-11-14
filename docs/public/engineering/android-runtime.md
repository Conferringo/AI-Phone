# Offline Android on desktop

The desktop installers carry the Android system and host runtime. Build inputs live in the factory, and the installed client starts Android from local files.

| Host | Android build | Host runtime |
| --- | --- | --- |
| Apple Silicon, macOS 15+ | [AOSP ARM64 16644040](https://ci.android.com/builds/submitted/16644040/aosp_cf_arm64_only_phone-userdebug/latest) | [Avo 1.1.0](https://github.com/nizwar/avo/releases/tag/v1.1.0), Apple Hypervisor |
| Windows 11 x64 | [AOSP x64 13455678](https://ci.android.com/builds/submitted/13455678/aosp_cf_x86_64_only_phone-userdebug/latest) | [QEMU 11.1.3](https://www.qemu.org/download/), Windows Hypervisor Platform |

Both use [ADB 36.0.1](https://github.com/meator/android-tools-static/releases/tag/36.0.1). The Android archive includes the platform and kernel source manifests, build metadata, component notices and SPDX inventory. See [corresponding source and rebuild instructions](android-corresponding-source.md) for the native VMM, Android and ADB. Each platform installs its first Android device into a separate data directory.

The Rust Android session owns launch, ADB and shutdown. The existing runner serves screen and input; on macOS it reads Avo's shared frame and PCM buffers. On Windows the existing authenticated Android bridge forwards ADB to the Linux service guest.

## Factory inputs

On a build host, stage the selected platform's clean SDK tree. macOS uses `avo/avo-vmm`, `avo/images`, and `platform-tools`. Windows uses `cuttlefish` for the boot kernel, initrd and base disks, QEMU and its dependencies; ADB is in `platform-tools`.

```sh
./scripts/bun scripts/android-component.ts --sdk-input /path/to/staged-sdk --work /path/to/new-capsule
```

Set `AIMANITY_ANDROID_SDK_INPUT` to this same staging directory when invoking a complete desktop factory. The built client uses only the resulting `android.payload`.
