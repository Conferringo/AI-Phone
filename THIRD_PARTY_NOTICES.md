# Third-party sources and licence status

Original AImanity code is licensed under **Apache-2.0**. Third-party software, data and assets retain their own licences, notices and source requirements.

## Distribution scope

The public **source export** contains project source, lockfiles, attribution records and selected upstream licence texts. It excludes SDK archives, guest images, prebuilt runtime components and personal data.

A **complete client bundle** includes the web application, native services, phone integration APKs, container host, guest applications, Android runtime and container images. See [runtime bundles](docs/public/runtime-bundles.md) and [release status](docs/public/release-status.md).

See [distribution requirements](docs/public/distribution-requirements.md) for the installer inventory and [offline Android runtime](docs/public/engineering/android-runtime.md) for the packaged sources.

## Components and attribution

| Component | Source / licence evidence | Distribution scope and obligations |
| --- | --- | --- |
| Static Linux ARM C runtime | [musl 1.2.5 copyright](docs/public/licenses/musl/COPYRIGHT), [Rust 1.96.0 build recipe and patches](https://github.com/rust-lang/rust/blob/1.96.0/src/ci/docker/scripts/musl.sh) | The Mac factory's Linux ARM desktop bridge uses Rust's musl target. The complete upstream copyright and terms are included in the native client and service notices |
| Runtime dependencies | Exact versions in Cargo.lock / bun.lock | [Inventory](docs/public/dependency-licenses.json): locked server and native-client build graphs, licence metadata and notice-file hashes. Redistribution/source-offer review remains open |
| EinkBro 16.7.1 Android browser | [Pinned release/source](config/phone-browser.json), [upstream notice](docs/public/licenses/guest-apps/einkbro-LICENSE.md), [GPL-3.0 text](docs/public/licenses/guest-apps/GPL-3.0.txt) | ARM64/x86_64 APKs are selected per guest and use the Android system WebView. Application data is excluded; embedded libraries retain their own notices and source requirements |
| Knowledge motion + native app icon | Original AImanity TypeScript/SVG implementations, Apache-2.0; hashes in config/source-assets.json | Original project implementation; source asset identities are recorded in the asset policy. |
| Native OCR test glyphs | DejaVu Sans; Bitstream Vera grant and public-domain DejaVu changes | Generated diagnostic PNG only, no font binary. [Font notice](docs/public/licenses/fixtures/dejavu.txt); font/image hashes in config/source-assets.json |
| Linux QEMU/SSH host | Pinned Ubuntu factory packages, QEMU 8.2.2, OpenSSH, BIOS/UEFI and ELF dependencies | [Host component policy](docs/public/container-host-runtime.md). Distribution copyright files, package versions and hashes retained locally; no binaries in source exports. Complete binary attribution and corresponding-source review remain open |
| Microsoft WSL 3.0.3.0 | [Pinned offline installer/source](config/windows-host.json), [WSL project MIT grant](docs/public/licenses/windows-wsl/LICENSE), [bundled package versions](https://github.com/microsoft/WSL/blob/3.0.3/packages.config) | The exact WSL Dependencies x64 NuGet package declares MIT. The MSI also includes MSRDC 1.2.7391, whose license restricts third-party distribution. Direct3D Linux and DXCore have no license declaration in their pinned archives. See the [Windows factory review](docs/public/windows-client-factory.md) |
| Container OCR | [Tesseract](https://github.com/tesseract-ocr/tesseract), Apache-2.0 · [Language data](config/ocr-models.json) · [Data licence](docs/public/licenses/tesseract-models/LICENSE) | The macOS workspace uses the bundled OCR container. Its distribution copyright files cover Tesseract and the native dependencies. |
| Offline speech recognition model | OpenAI Whisper tiny, Q5_1 conversion from [ggerganov/whisper.cpp](https://huggingface.co/ggerganov/whisper.cpp/tree/5359861c739e955e79d9a303bcbc70fb988958b1); [MIT licence](docs/public/licenses/whisper/LICENSE), [download pin](config/speech-model.json) | Both clients include the multilingual model at `host/speech/ggml-tiny-q5_1.bin` and retain the upstream copyright and permission notice |
| Whisper Rust bindings | whisper-rs 0.16.0 and whisper-rs-sys 0.15.0, [source revision](https://codeberg.org/tazz4843/whisper-rs/src/commit/7558e1b72f54f2f22a53589afb77e65681834c36), [Unlicense](docs/public/licenses/native/whisper-rs-LICENSE) | Native clients statically link the Rust bindings and retain their shared public-domain dedication |
| whisper.cpp and ggml | Vendored in the [whisper-rs-sys 0.15.0 crate](https://crates.io/crates/whisper-rs-sys/0.15.0), [MIT licence](docs/public/licenses/native/whisper.cpp-LICENSE), copyright 2023–2024 The ggml authors | Native clients statically link this inference code and include its original copyright and permission notice |
| Windows offline OCR | [Ubuntu Tesseract 5.3.4-1build5 source](https://archive.ubuntu.com/ubuntu/pool/universe/t/tesseract/tesseract_5.3.4-1build5.dsc) · [English and Simplified Chinese data 1:4.1.0-2 source](https://archive.ubuntu.com/ubuntu/pool/universe/t/tesseract-lang/tesseract-lang_4.1.0-2.dsc) | The Windows Linux service bundles the native ELF dependencies, language data, package copyright files and `runtime/services/ocr/source-records.json`. The record lists copied native files and factory package versions. [Factory](docs/public/engineering/linux-offline-ocr.md) · [Ubuntu archive snapshot](https://snapshot.ubuntu.com/ubuntu/20261031T040000Z/) |
| Offline Android | [AOSP, Avo, QEMU and ADB sources](docs/public/engineering/android-runtime.md) | The Android capsule includes original notices, source and build manifests, and kernel configuration. |
| Android playback on Windows | [scrcpy server 3.3.4](https://github.com/Genymobile/scrcpy/tree/v3.3.4), [Apache-2.0](https://github.com/Genymobile/scrcpy/blob/v3.3.4/LICENSE) | The Android capsule includes Genymobile’s unmodified server and its licence. |
| mise 2026.10.22 | [Official source / MIT](https://github.com/jdx/mise/tree/v2026.10.22) | Managed-host development downloads the pinned executable separately; not bundled in the client. Retain upstream notices in the installed tool distribution |
| Lima 2.2.1 and Alpine 3.24.1 | [Lima source and licence](https://github.com/lima-vm/lima/tree/27bc4c4b101e27994e01e36566a0f2091bbca507) · [Alpine-Lima v0.2.50 image build](https://raw.githubusercontent.com/lima-vm/alpine-lima/v0.2.50/Makefile) · [Alpine package build recipes](https://github.com/alpinelinux/aports) | The macOS bundle includes Lima’s Apache-2.0 licence, the ARM64 ISO and 31 signed Alpine packages. Docker 29.9.3-r1, Compose 5.6.1-r0 and runc 1.4.4-r0 are listed in the [package policy](config/container-host-runtime.json). The ISO boots Linux 6.18.38-0-virt. |
| Go 1.26.9 factory compiler | [Go source / BSD-3-Clause and notices](https://go.googlesource.com/go/+/refs/tags/go1.26.9/LICENSE) | Maintainer-only private compiler; never a global user prerequisite. Preserve Go/runtime/module notices in the resulting Lima distribution |
| Extra Android apps | Separately installed APKs | No APK redistribution in source bundles; pin source, version, signature and licence before automating downloads |
| Docker images and guest packages | Pinned bases / Debian and package-manager dependencies | Preserve package notices and redistribution obligations in binary/image releases |
| Dunst and libnotify | [Dunst / BSD-3-Clause](https://github.com/dunst-project/dunst), [libnotify / LGPL-2.1-or-later](https://gitlab.gnome.org/GNOME/libnotify) | Included in the Linux desktop image for notifications. Retain distribution copyright files and applicable source/redistribution obligations |
| Redis 8 Compose image | [Redis licence options](https://redis.io/legal/licenses/), AGPLv3, RSALv2 or SSPLv1 | Optional runtime service in the container-image capsule. The Agent queue uses PostgreSQL. Record the licence selected for the exact image and meet its source obligations |
| Optional contributor Docker Desktop | [Docker Subscription Service Agreement overview](https://docs.docker.com/subscription-billing/desktop-license/) | Contributors install Docker Desktop separately under Docker's terms |
| Mihomo v1.19.33 | [Corresponding source and GPLv3 licence](https://github.com/MetaCubeX/mihomo/tree/v1.19.33), [platform pins and licence](config/mihomo-release.json) | The offline macOS and Windows runtime bundles include the unmodified Darwin ARM64 and Linux x64 cores respectively. The workbench proxy starts its core on demand. |
| GeoIP routing data | [MetaCubeX/meta-rules-dat](https://github.com/MetaCubeX/meta-rules-dat), immutable commit and hash in config/mihomo-release.json | Downloaded separately, excluded from source archives; retain upstream licence and data-source obligations in binary distribution |

Binary releases include the component notices and applicable corresponding-source references.

GSAP 3.15.0 uses its own [Standard No Charge licence](https://gsap.com/community/standard-license/). The source depends on the separately installed package and preserves its notices. Its restrictions on competing visual animation builders remain applicable. Native libvips (LGPL), lightningcss/@vercel/og (MPL), caniuse-lite data (CC BY) and mixed-licence packages remain identified in the inventory.

The native client bundles collected licence texts in `docs/public/native-client-licenses.txt`. The locked notice inventory reports every required file present.

The 40 native-client notice supplements record source URLs, exact revisions or archive checksums, and licence text hashes in `config/native-notice-supplements.json`. The objc2 grant includes the original Apple SDK provision, MIT text and attribution.

## Source availability and Linux system libraries

The Alpine ISO lists package names, licences and aports commits in `apks/aarch64/APKINDEX.tar.gz`. Each separate APK records its `origin` and `commit` in `.PKGINFO`. Use those fields to open the corresponding `APKBUILD` and its patches in aports. [BusyBox 1.37.0-r31 build recipe](https://github.com/alpinelinux/aports/blob/c3ef5d10e6ef6528852c51f0564963e2f8c1be19/main/busybox/APKBUILD)

For MPL-2.0 client dependencies such as selectors/cssparser, exact crate versions and public source URLs are retained in the notice collection and Cargo.lock. selectors corresponding source is https://crates.io/api/v1/crates/selectors/0.36.1/download (checksum in the supplement record). The unmodified source can be downloaded and rebuilt; any in-repository native patch is separately preserved with its upstream license/source under apps/client/src-tauri/vendor.

Complete installers must include these attribution texts together with the notices and source access required by their runtime images and server dependencies. Signing and installation acceptance are reported separately in release status. Component terms and copyright notices apply independently of Apache-2.0.

## Rust standard-library runtime

The clients are built with Rust 1.96.0. Its standard-library copyright collection and 13 accompanying licence/attribution files are pinned in config/rust-runtime-notices.json, verified against the installed official component manifest and supplied in the native notice collection. The full compiler is not bundled. Changing the toolchain requires a runtime-notice review.

### Location interface

The map uses Leaflet 1.9.4 under BSD-2-Clause. Its full [copyright and license](docs/public/licenses/javascript/leaflet/LICENSE) are retained. OpenStreetMap map data is attributed in the map and governed by [ODbL](https://www.openstreetmap.org/copyright); hosted tiles follow the provider's [tile usage policy](https://operations.osmfoundation.org/policies/tiles/). Tiles and the optional network-location lookup are contacted only after simulated location is enabled. No FakeLocation source or application binary is bundled.

## Local WhatsApp connection

WhatsApp uses the MIT-licensed `whatsapp-rust` 0.7.0 linked-device implementation. Its transitive dependencies and required notices must be included in the generated binary inventories.

The linked-device adapter is unofficial. The platform's terms apply to pairing and delivery. Session databases, linked-device state and QR payloads remain private user data. Chromium in the Linux desktop retains its own notices and source obligations.

The locked archive dependencies use glob 14.0.3 and minimatch 11.0.3, covering [glob cancellation](https://github.com/advisories/GHSA-3g3m-2fjr-6pw4) and [minimatch pattern amplification](https://github.com/advisories/GHSA-g3hv-75wg-m249). The SDK archive directory/pattern integration is tested with Unicode, hidden files and ignore rules.

## Web and installer runtimes

Complete clients include Next.js standalone output and pinned Bun, with Rust host initialization. Windows runs the web and Rust services in its bundled Linux guest. The web, native and image components each have their own notices and source requirements. See [runtime bundles](docs/public/runtime-bundles.md).

The client uses the WebView2 Runtime included in Windows 11 under Microsoft's terms. See [Microsoft's WebView2 documentation](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/evergreen-vs-fixed-version).

Windows installers are produced by unmodified Inno Setup 7.1.1. Its original installer copyright and notices are preserved; the [full upstream grant](docs/public/licenses/windows-installer/LICENSE) is included in the source. Compiler and InnoUnp 2.71.3 source/download identities are pinned in config/windows-installer.json. The build enables the pinned x86 unpacker’s PE large-address flag after archive verification. The compiler and unpacker are maintainer tools, excluded from the installed application runtime.

## Media and channel runtime component

The frontend uses the pinned Bun runtime and preserves its locked production dependency notices. First-party messaging and media services are Rust. Guest Chromium and Clash applications retain their own redistribution requirements.

## Native host and guest services

The Linux phone bridge includes a verbatim AOSP `emulator_controller.proto` schema from `platform/prebuilts/android-emulator` commit `bbe98768a47ce9166f768e791768ae5f066c04df`. Its original 2018 Android Open Source Project copyright and Apache-2.0 header are preserved. [Exact provenance](packages/protocol/vendor/android-emulator/provenance.json) records the upstream URL, Git blob identity, byte count and SHA-256 digest. The Linux phone bridge uses the independent Android tools release, with its corresponding source information and notice.

The [native service licence collection](docs/public/native-services-licenses.txt) and [inventory](docs/public/native-services-license-inventory.json) identify the locked Rust service dependencies, full terms, copyright notices and source references. The Linux guest image retains these materials under `/usr/share/doc/ai-phone/`.

Image processing enables PNG in image 0.25.10. The x11rb protocol bindings retain the separate xcb-proto copyright and permission notice. The host archive decoder links liblzma through xz2 and lzma-sys; the collection preserves the vendored XZ terms separately from the Rust wrapper licences. XZ command-line programs and shell utilities are not part of this static library build. This software includes code from [XZ Utils](https://tukaani.org/xz/).
