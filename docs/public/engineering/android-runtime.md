# Android runtime component

AI Phone separates its Android system/runtime from its small source archive. An optional, host-matched `android.payload` travels as a native application resource. The client verifies the resource digest before the installer extracts it into `components/android/<sha256>/sdk`. Device data remains in the installation's user directory.

The component contains emulator executables, platform tools and an Android factory image. It never includes an existing AVD, account, application database, chat or ADB identity. The image archive is checked before packaging; file sizes, SHA-256 digests, executable permissions and host/ABI are bound by its manifest. Extraction accepts regular, inventoried files only, with separate limits of 20,000 files, 4 GiB per file and 8 GiB total, plus a 4 GiB recovery reserve. The source archive's existing limits remain unchanged.

Initialization uses the selected CPU, memory and storage presets to create an AVD offline. A complete existing AVD is preserved; an incomplete one stops creation for recovery. A bundled SDK bypasses SDK Manager, AVD Manager and Java installation. This does **not** yet make every service, Linux image, application, browser or platform runtime independent of downloads.

## Engineering workflow

Factory inputs are recorded in [android-factory-images.json](../../../config/android-factory-images.json). Runtime construction is a maintainer operation; users of a complete distribution should not need this workflow.

```sh
python3 scripts/android_runtime.py build \
  --sdk /path/to/reviewed/sdk \
  --image-archive /path/to/verified/factory.zip \
  --image-sha256 <recorded-sha256> \
  --output /path/to/android.payload

python3 scripts/build_client.py --platform macos \
  --with-web-runtime --with-service-runtime \
  --android-runtime /path/to/android.payload
```

The build passes one generated resource map to Tauri; artifact inspection uses the same map. Resource hashing and expanded native-file inspection stream bytes instead of loading a system image into memory. Capsule preparation verifies the manifest and host; installation verifies every extracted body before activation. Source-only builds exclude this optional resource and discard a previously prepared capsule.

## Evidence and limits

On macOS ARM64, an official Android 35 revision 2 image and the locally reviewed emulator/platform-tools baseline produced a 385-file, 3,005,776,039-byte development component. Its extraction succeeded inside an OS sandbox denying network access. An isolated emulator then completed cold boot in approximately 24 seconds, returned root UID 0, accepted input, produced a 720×1440 screenshot and preserved a test file across restart. The emulator used a diagnostic data disk of approximately 6 GiB; **this is not acceptance of the 16 GiB product preset**. A separate 16 GiB attempt correctly stopped when the host lacked the emulator's required free space. The 35 GiB fresh-install gate remains unchanged.

Real boot evidence currently covers macOS ARM64 only. Linux x64 packaging/creation uses the same admission contract but has no equivalent boot acceptance in this batch. Windows and privileged Android independent hosts remain incomplete. An Android-host ARM64 guest is distinct from the macOS ARM64 emulator binary and cannot reuse it.

The development component is **not approved for public redistribution**. SDK terms, individual open-source notices, emulator/Qt/QEMU components and corresponding-source obligations need component-specific review. A successful local package or boot does not satisfy that release gate. See [Android SDK terms](https://developer.android.com/studio/terms) and the [AOSP licensing overview](https://source.android.com/docs/setup/contribute/licenses).

---

## 中文

Android 镜像与模拟器采用独立组件，随原生客户端资源传入。安装器校验宿主、ABI、清单、大小、权限和摘要后激活；设备数据仍保存在用户目录。现有完整设备不覆盖，不完整状态会停止并保留。离线设备创建不调用 Java、SDK Manager 或 AVD Manager。其余服务、Linux 镜像和各平台完整运行环境尚未全部离线封装。

本批已在 macOS ARM64 真实验证：禁止网络时展开；干净 Android 冷启动约 24 秒，root、输入、720×1440 画面和重启后文件保留通过。诊断盘实际约 6GiB，不能代替 16GB 生产预设验收；后者因磁盘容量要求停止。35GiB 全新部署门槛保持。Linux 的对应启动验证、Windows 与 root/KVM Android 独立宿主及完整原生安装仍有待完成。

该开发组件仍需逐项完成再分发声明和对应源码审查，未作为正式安装包公开。运行验证不代表全平台或完整产品通过。
