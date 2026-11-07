# Android distribution review

Reviewed 2026-11-07. This records the exact stock inputs used by complete-client factories and the remaining corresponding-source work. It does not approve binary publication or change a running installation.

## Stock inputs

[The factory policy](../../../config/android-factory.json) pins each archive by URL, byte count and SHA-256. The current complete Mac and Windows factory receipts identify Emulator **37.2.13 / build 16740729** and platform tools **37.0.1 / build 15733141**. They use API 35 default AOSP system images, revision 2, with their matching guest ABI. These are separate components with separate source obligations.

| Component examined | Direct archive evidence | Remaining qualification |
| --- | --- | --- |
| Linux x64 Emulator 37.2.13 | The exact 349,658,551-byte policy archive was checked. Its `source.properties` identifies build 16740729; `LICENSE`, `NOTICE.txt` and `NOTICE.csv` are present. The CSV maps 64 external-library rows to actual executables/libraries. | Bind this build to immutable full-project revisions, build settings, dependency sources and changes; preserve every applicable notice and source/relinking obligation. This examination does not cover the Mac/Windows archive bodies. |
| macOS platform tools 37.0.1 | The exact 16,110,554-byte policy archive was checked. Its `source.properties` identifies 37.0.1 and its `NOTICE.txt` contains 1,152,421 bytes of component notices. | Resolve the exact shipped tools and their dependency sources/terms. An ADB version number or the project licence does not cover every other program in the archive. |
| API 35 AOSP images, revision 2 | Architecture-specific archives are pinned by the factory policy. Factory receipts bind them to the packaged capsule. | Review the kernel, ramdisk, system/vendor images, bundled programs and their matching source/notice delivery separately. Factory admission is not corresponding-source evidence. |

The inspected Linux emulator's notice map includes QEMU and e2fs GPL-2.0 components, FFmpeg/dtc GPL-family components, Qt LGPL-3.0 components, GLib LGPL-2.1, and permissive components. Its short top-level Apache-2.0 `LICENSE` therefore cannot describe the whole distribution. Preserve the original package notices; do not infer one licence for every contained file.

One upstream CSV row labels libselinux `CC-PDDC OR UNKNOWN`. The linked upstream libselinux notice explicitly declares public-domain software and includes warranty/liability terms. That narrows this row's ambiguity; it does not bind a moving source branch to the exact distributed binary or clear other components.

## Source identity

A source build of a different emulator version does not qualify the stock archives. Each distributed component needs an immutable source/build binding, required notices and applicable corresponding source. The binding for the current stock emulator remains unresolved.

The official `emu-master-dev` and `emu-main-dev` branches describe different moving source selections. Their current files, a nearby release tag, or a successful HTTP response containing an Artifact Viewer page do not prove which sources produced build 16740729. The immutable release-build binding is still unresolved. If a new source-built component replaces a stock artifact, it needs a new explicit identity and its own runtime checks; existing user devices are preserved.

## Applicable terms

[Android SDK terms §§3.4–3.5](https://developer.android.com/studio/terms) distinguish the SDK grant from open-source components, which follow their own licences. This calls for component review, rather than treating every SDK byte as either forbidden or automatically cleared. Original AI Phone Apache-2.0 code does not relicense third-party binaries or trademarks.

Complete publication requires the notices, actual build/source identity and applicable corresponding-source delivery for each shipped component. The current `redistribution_review_complete` flags stay false while that evidence is incomplete. See [distribution requirements](../preview-release.md) and [third-party scope](../../../THIRD_PARTY_NOTICES.md).

## 中文

当前完整工厂使用模拟器37.2.13/build16740729、platform-tools37.0.1/build15733141和API35 revision2的AOSP镜像。已直接核对固定Linux模拟器归档及Mac platform-tools归档；模拟器含64条库与程序映射，涵盖GPL、LGPL及宽松许可，不能只看顶层Apache声明。

现有源码工厂固定的是35.3.8/build12560773，现已完成一次真实Linux源码构建，保留304324177字节的SDK候选；API35 root启动未执行、源码依赖闭包未验证、再分发审阅未完成。该构建不能替代37.2.13的对应源码，也没有替换现有设备。精确发布构建与完整源码/依赖/构建参数的绑定仍未闭合；系统镜像的内核及系统程序也须单独核对。libselinux上游声明已澄清一项UNKNOWN标签，未据此批准整包。保持原声明和来源版本，不翻发行标志、不替换现有设备；完整安装器构建成功与许可/对应源码通过分别记录。
