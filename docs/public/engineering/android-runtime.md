# Android runtime component

AI Phone separates its Android system/runtime from its small source archive. An optional, host-matched `android.payload` travels as a native application resource. The client verifies the resource digest before the installer extracts it into `components/android/<sha256>/sdk`. Device data remains in the installation's user directory.

The component contains emulator executables, platform tools and an Android factory image. It never includes an existing AVD, account, application database, chat or ADB identity. The image archive is checked before packaging; file sizes, SHA-256 digests, executable permissions and host/ABI are bound by its manifest. Extraction accepts regular, inventoried files only, with separate limits of 20,000 files, 4 GiB per file and 8 GiB total, plus a 4 GiB recovery reserve. The source archive's existing limits remain unchanged.

New source and Android capsules use XZ compression. Existing gzip capsules remain readable, and the content-addressed component directories keep old installations separate from newly admitted resources.

The complete Mac factory uses XZ preset 9; the general component builder retains preset 6, and component-only Mac validation selects preset 3 to reduce encoding work. These profiles preserve the exact uncompressed SDK/TAR bytes and use the same admission and executable checks. The selected profile and actual archive digest are recorded by the producer. The decoder's 128 MiB memory limit is unchanged.

In a read-only measurement of the `e9ecab37fbcb` Mac factory, preset 9 reduced the Android capsule from 867,495,492 to 785,368,004 bytes. All 3,118,755,840 uncompressed bytes matched by SHA-256 after round-trip decoding within that limit. Factory encoding took about 680 seconds.

Initialization uses the selected CPU, memory and storage presets to create an AVD offline. A complete existing AVD is preserved; an incomplete one stops creation for recovery. A bundled SDK bypasses SDK Manager and AVD Manager. Java is omitted only when the [prebuilt control applications](phone-apps.md) are also included.

## Engineering workflow

Factory inputs are recorded in [android-factory-images.json](../../../config/android-factory-images.json). Runtime construction is a maintainer operation; users of a complete distribution should not need this workflow.

```sh
./scripts/bun scripts/android-component.ts --work /path/to/new-android-component

./scripts/bun scripts/build-client.ts --platform macos \
  --with-web-runtime --with-service-runtime --with-phone-apps \
  --android-runtime /path/to/new-android-component/android.payload
```

The build passes one generated resource map to Tauri; artifact inspection uses the same map. Resource hashing and expanded native-file inspection stream bytes instead of loading a system image into memory. Capsule preparation verifies the manifest and host; installation verifies every extracted body before activation. Source-only builds exclude this optional resource and discard a previously prepared capsule.

## Availability

Current platform support, installer availability and distribution requirements are maintained in [release status](../release-status.md).

Factory artifacts remain subject to their own SDK terms, open-source notices and corresponding-source requirements. See [Android SDK terms](https://developer.android.com/studio/terms) and the [AOSP licensing overview](https://source.android.com/docs/setup/contribute/licenses).

---

## 中文

Android 镜像与模拟器采用独立组件，随原生客户端资源传入。安装器校验宿主、ABI、清单、大小、权限和摘要后激活；设备数据仍保存在用户目录。现有完整设备不覆盖，不完整状态会停止并保留。离线设备创建不调用 Java、SDK Manager 或 AVD Manager。其余服务、Linux 镜像和各平台完整运行环境尚未全部离线封装。

模拟器必须与桌面宿主的系统和架构匹配。当前平台供应、默认资源要求和首次安装范围统一见[发行状态](../release-status.md)。

该开发组件仍需逐项完成再分发声明和对应源码审查，未作为正式安装包公开。
