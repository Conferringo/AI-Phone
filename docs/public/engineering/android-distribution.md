# Android distribution review

The 0.1.0 candidate factories use the stock inputs below.

## Stock inputs

[The factory policy](../../../config/android-factory.json) pins each archive by URL, byte count and SHA-256. The current complete Mac and Windows factory receipts identify Emulator **37.2.13 / build 16740729** and platform tools **37.0.1 / build 15733141**. They use API 35 default AOSP system images, revision 2, with their matching guest ABI. These are separate components with separate source obligations.

| Component examined | Direct archive evidence | Remaining qualification |
| --- | --- | --- |
| Linux x64 Emulator 37.2.13 | The exact 349,658,551-byte policy archive was checked. Its `source.properties` identifies build 16740729; `LICENSE`, `NOTICE.txt` and `NOTICE.csv` are present. The CSV maps 64 external-library rows to actual executables/libraries. | Bind this build to immutable full-project revisions, build settings, dependency sources and changes; preserve every applicable notice and source/relinking obligation. Inspect the separate Mac and Windows emulator archives. |
| macOS platform tools 37.0.1 | The exact 16,110,554-byte policy archive was checked. Its `source.properties` identifies 37.0.1 and its `NOTICE.txt` contains 1,152,421 bytes of component notices. | Resolve the exact shipped tools and their dependency sources/terms. |
| API 35 AOSP images, revision 2 | Both kernels and all 89 vendor kernel modules match Google's published build 11987101 byte for byte. The original kernel configurations were extracted and the source archive for `dd9c02ccfe27` was verified. | Resolve the exact module build graph and the system, vendor userspace and ramdisk sources for image build 12368160. |

The inspected Linux emulator's notice map includes QEMU and e2fs GPL-2.0 components, FFmpeg/dtc GPL-family components, Qt LGPL-3.0 components, GLib LGPL-2.1, and permissive components. Preserve the package’s original component notices and terms.

One upstream CSV row labels libselinux `CC-PDDC OR UNKNOWN`. The linked upstream libselinux notice explicitly declares public-domain software and includes warranty/liability terms. Record the exact revision compiled into the binary.

## Source identity

The stock emulator, platform tools and API 35 images still need build manifests that tie their exact versions to source revisions and build recipes.

AOSP's [emulator image build](https://source.android.com/docs/setup/test/avd) supports a source-built replacement. The [Android 15 r14 manifest](https://android.googlesource.com/platform/manifest/+/380eb7938773a2fc95a371e84e93436eaf4090c3) and [Goldfish products](https://android.googlesource.com/device/generic/goldfish/+/157a6de4150e70fb8031c89a76bd2335bdab8a35/AndroidProducts.mk) identify ARM64 and x64 image targets. Both targets use prebuilt 6.6 kernels and modules by default. A replacement needs corresponding kernel and module source, host-specific emulator and ADB builds, and verification of the project's device control and media interfaces.

## Applicable terms

[Android SDK terms §§3.4–3.5](https://developer.android.com/studio/terms) distinguish the SDK grant from open-source components, which follow their own licences.

Complete publication requires the notices, actual build/source identity and applicable corresponding-source delivery for each shipped component. The current `redistribution_review_complete` flags stay false while that evidence is incomplete. See [distribution requirements](../distribution-requirements.md) and [third-party scope](../../../THIRD_PARTY_NOTICES.md).

## 中文

当前完整工厂使用模拟器37.2.13/build16740729、platform-tools37.0.1/build15733141和API35 revision2的AOSP镜像。已直接核对固定Linux模拟器归档及Mac platform-tools归档；模拟器含64条库与程序映射，涵盖 GPL、LGPL 及宽松许可。

精确发布构建与完整源码、依赖及构建参数的绑定仍未闭合；系统镜像的内核及系统程序也须单独核对。各归档需要其实际构建版本的对应源码。libselinux 上游声明为公有领域，仍须核对实际构建版本。
