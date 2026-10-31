# Third-party sources and licence status

Original AI Phone code is licensed under **Apache-2.0**. Third-party software, data and assets retain their own licences, notices and source requirements.

## Distribution scope

The public **source export** contains project source, lockfiles, attribution records and selected upstream licence texts. It excludes SDK archives, guest images, prebuilt runtime components and personal data.

A **complete client bundle** includes the web application, native services, phone integration APKs, container host, JavaScript services, guest applications, Android runtime, container images and installer tools. The compiled macOS and Linux bundles contain these nine resource groups, including server binaries and Android/Linux baselines. Their inventories and qualification are documented in [runtime bundles](docs/public/runtime-bundles.md) and [release status](docs/public/release-status.md). Those package contents have a broader attribution and corresponding-source scope than the source export.

Public binary distribution remains pending the [release requirements](docs/public/preview-release.md). A successful build, file hash or copied licence text does not establish redistribution permission. Component review flags remain authoritative until the exact shipped binaries, notices and applicable corresponding-source delivery are qualified.

## Components and attribution

| Component | Source / licence evidence | Distribution scope and obligations |
| --- | --- | --- |
| Runtime dependencies | Exact versions in Cargo.lock / bun.lock | [Inventory](docs/public/dependency-licenses.json): 572 Rust and 425 JavaScript packages across the server and native client, licence metadata and notice-file hashes. One legacy package (async 0.2.10) omits its SPDX field; its packaged MIT grant is retained and hashed. Redistribution/source-offer review remains open |
| Knowledge motion + native app icon | Original AI Phone TypeScript/SVG implementations, Apache-2.0; hashes in config/source-assets.json | Original project implementation; source asset identities are recorded in the asset policy. |
| Native OCR test glyphs | DejaVu Sans; Bitstream Vera grant and public-domain DejaVu changes | Generated diagnostic PNG only, no font binary. [Font notice](docs/public/licenses/fixtures/dejavu.txt); font/image hashes in config/source-assets.json |
| Linux QEMU/SSH host | Pinned Ubuntu factory packages, QEMU 8.2.2, OpenSSH, BIOS/UEFI and ELF dependencies | [Host component policy](docs/public/container-host-runtime.md). Distribution copyright files, package versions and hashes retained locally; no binaries in source exports. Complete binary attribution and corresponding-source review remain open |
| Linux offline OCR | Tesseract 5.3.4, English/Chinese data, Pillow 13.0.0 and distribution ELF dependencies | [Factory and component policy](docs/public/engineering/linux-offline-ocr.md). Local component retains distribution copyright files, wheel notices, package versions and file identities. Source exports contain no OCR binaries; full binary licence/corresponding-source review remains open |
| Android SDK/emulator/system image | Google's SDK packages | Included in complete runtime bundles, excluded from source exports. Review the exact emulator, platform tools and image separately under their applicable terms and open-source licences; corresponding-source and redistribution qualification remain pending |
| mise 2026.10.22 | [Official source / MIT](https://github.com/jdx/mise/tree/v2026.10.22) | Managed-host development downloads the pinned executable separately; not bundled in the client. Retain upstream notices in the installed tool distribution |
| Lima 2.2.1 and container-host image | [Lima source / Apache-2.0](https://github.com/lima-vm/lima/tree/v2.2.1), [Alpine-Lima source](https://github.com/lima-vm/alpine-lima/tree/v0.2.50) | Complete macOS/Linux bundles include the matched Lima runtime, Alpine ISO and signed Docker/Compose package closure. Retain every included package notice and applicable corresponding source; full distribution qualification remains pending |
| Go 1.26.9 factory compiler | [Go source / BSD-3-Clause and notices](https://go.googlesource.com/go/+/refs/tags/go1.26.9/LICENSE) | Maintainer-only private compiler; never a global user prerequisite. Preserve Go/runtime/module notices in the resulting Lima distribution |
| Clash Meta for Android 2.11.36 | [Official source / GPLv3](https://github.com/MetaCubeX/ClashMetaForAndroid/tree/v2.11.36); APK SHA256 and signature pin in config/clash-apps.json | Included as a guest application resource in complete bundles; excluded from source exports. Preserve upstream notices and applicable corresponding source |
| Clash Party 2.0.4 | [Official source / GPLv3](https://github.com/mihomo-party-org/clash-party/tree/v2.0.4); deb pin in config/clash-apps.json | Unmodified package in Linux guest image; retain upstream licences and fulfil corresponding-source requirements before redistributing images |
| Extra Android apps | Separately installed APKs | No APK redistribution in source bundles; pin source, version, signature and licence before automating downloads |
| Docker images and guest packages | Pinned bases / Debian and package-manager dependencies | Preserve package notices and redistribution obligations in binary/image releases |
| Dunst and libnotify | [Dunst / BSD-3-Clause](https://github.com/dunst-project/dunst), [libnotify / LGPL-2.1-or-later](https://gitlab.gnome.org/GNOME/libnotify) | Included in the Linux desktop image for notifications. Retain distribution copyright files and applicable source/redistribution obligations |
| Redis 8 Compose image | [Redis licence options](https://redis.io/legal/licenses/): AGPLv3, RSALv2 or SSPLv1, not the historic BSD terms | Included in the complete container-image capsule as an optional runtime service; the Agent queue uses PostgreSQL. Record the selected licence option and exact image/source obligations. The source package inventory alone does not cover this image |
| Optional contributor Docker Desktop | [Docker Subscription Service Agreement overview](https://docs.docker.com/subscription-billing/desktop-license/) | Not bundled or relicensed by this project. User eligibility/acceptance is separate from Apache-2.0; the installer must not imply unrestricted free enterprise use |
| Mihomo v1.19.33 | [Corresponding source and GPLv3 licence](https://github.com/MetaCubeX/mihomo/tree/v1.19.33); hashes in config/mihomo-release.json | Installed from verified official releases; unmodified core, generated private configuration. Binaries are excluded from source archives. Binary/image releases must include required notices and corresponding-source access |
| GeoIP routing data | [MetaCubeX/meta-rules-dat](https://github.com/MetaCubeX/meta-rules-dat), immutable commit and hash in config/mihomo-release.json | Downloaded separately, excluded from source archives; retain upstream licence and data-source obligations in binary distribution |

The Apache-2.0 project licence does not override third-party, copyleft, trademark or asset obligations. The source export and complete client bundle have separate inventories. Binary/image redistribution requires notices and applicable corresponding source for the exact included components. Generated assets and model names do not imply official affiliation.

GSAP 3.15.0 uses its own [Standard No Charge licence](https://gsap.com/community/standard-license/), not Apache/MIT or an OSI-approved project licence. The source depends on the separately installed package and preserves its notices. Its restrictions on competing visual animation builders remain applicable. Native libvips (LGPL), lightningcss/@vercel/og (MPL), caniuse-lite data (CC BY) and mixed-licence packages remain identified in the inventory.


The native client bundles collected licence texts in `docs/public/native-client-licenses.txt`. Complete text references and upstream grants for omitted crate notices are pinned in config/native-notice-supplements.json. The current client inventory reports no missing notice files. Source grants, full terms, copyright attributions and source availability remain distinct from a blanket legal certification.

Native-client notice supplements cover 40 locked crates: exact VCS grants, verified published-archive grants, and full terms linked by upstream grants. Paths, source URLs, revisions/archive checksums and SHA256 are recorded in `config/native-notice-supplements.json`; no missing client notice files remain. The objc2 upstream grant/Apple-SDK caveat is preserved alongside the full MIT text and author attribution; this is not a claim to relicense Apple SDKs. Host operating-system frameworks are supplied by the host. The bundled Android SDK/emulator/image and guest applications belong to their respective runtime inventories; the Rust-client notice collection does not cover them.

## Android Java/Kotlin runtime

`docs/public/android-client-licenses.txt` includes all 79 resolved runtime/BOM coordinates, Apache-2.0 terms and upstream embedded LICENSE/NOTICE attributions (including shaded FastDoubleParser notices). POM/notice files and hashes are in `config/android-dependency-notices.json` and `docs/public/licenses/android/`. Packaging compares the resolved release graph before accepting/signing the built artifact. The Android host application uses the host device’s system and WebView. A bundled virtual Android guest has a separate system-image inventory and licensing scope.

## Source availability and Linux system libraries

For MPL-2.0 client dependencies such as selectors/cssparser, exact crate versions and public source URLs are retained in the notice collection and Cargo.lock. selectors corresponding source is https://crates.io/api/v1/crates/selectors/0.36.1/download (checksum in the supplement record). The unmodified source can be downloaded and rebuilt; any in-repository native patch is separately preserved with its upstream license/source under apps/client/src-tauri/vendor. Linux installers dynamically use the distribution GTK/WebKit/system libraries rather than bundling their binaries; install those through the distribution package manager.

Complete installers must include these attribution texts together with the notices and source access required by their runtime images and server dependencies. Signing and installation acceptance are reported separately in release status. Component terms and copyright notices apply independently of Apache-2.0.

## Rust standard-library runtime

The clients are built with Rust 1.96.0. Its standard-library copyright collection and 13 accompanying licence/attribution files are pinned in config/rust-runtime-notices.json, verified against the installed official component manifest and supplied in the native notice collection. The full compiler is not bundled. Changing the toolchain requires a runtime-notice review.

### Location interface

The map uses Leaflet 1.9.4 under BSD-2-Clause. Its full [copyright and license](docs/public/licenses/javascript/leaflet/LICENSE) are retained. OpenStreetMap map data is attributed in the map and governed by [ODbL](https://www.openstreetmap.org/copyright); hosted tiles follow the provider's [tile usage policy](https://operations.osmfoundation.org/policies/tiles/). Tiles and the optional network-location lookup are contacted only after simulated location is enabled. No FakeLocation source or application binary is bundled.

## Local WhatsApp connection

Puppeteer is pinned to 25.16.0 in the root dependency policy. The local linked-device integration and browser lifecycle are described in [the browser runtime guide](docs/public/engineering/channel-browser.md).

The optional local linked-device connection uses **whatsapp-web.js 1.34.7 (Apache-2.0)** and **qrcode 1.5.4 (MIT)**. Full upstream terms are retained in [whatsapp-web.js](docs/public/licenses/javascript/whatsapp-web.js/LICENSE) and [qrcode](docs/public/licenses/javascript/qrcode/license). Its legacy async 0.2.10 transitive dependency ships a [MIT grant](docs/public/licenses/javascript/async/LICENSE) without a package SPDX field. The dependency inventory records that missing metadata rather than inventing a declaration. WhatsApp and Meta do not sponsor this project; the Web protocol adapter is unofficial.

Bundled installations use Chromium from the included Linux desktop image for fresh accounts. Existing host profiles and contributor overrides retain their original host browser. Complete binary distributions require the image’s full Chromium third-party notices, source obligations and provenance; binary publication awaits that review. No personal browser profile, linked-device state or QR payload belongs in a source export. See the [runtime policy](docs/public/engineering/channel-browser.md).

The locked archive dependencies use glob 14.0.3 and minimatch 11.0.3, covering [glob cancellation](https://github.com/advisories/GHSA-3g3m-2fjr-6pw4) and [minimatch pattern amplification](https://github.com/advisories/GHSA-g3hv-75wg-m249). The SDK archive directory/pattern integration is tested with Unicode, hidden files and ignore rules. Licence metadata and packaged upstream grants remain independent of the advisory result.

## Web and installer runtimes

Complete macOS/Linux bundles include Next.js standalone output and the pinned Bun executable. Installer tools add portable Python and Node. Their runtime licences, third-party attribution and applicable source requirements are separate from the native Rust/WebView client graph. The JavaScript inventory identifies application packages; it does not by itself cover every native runtime or image dependency. See [runtime bundle boundaries](docs/public/runtime-bundles.md) and [installer tools](docs/public/engineering/installer-tools.md).

## Media and channel runtime component

The bundled JavaScript service component retains locked production package files, including their upstream licence texts, and uses the pinned Bun runtime. Media capture uses Sharp and libvips at runtime. Full native library dependency notices and applicable corresponding-source delivery remain binary publication requirements. An exact registry-provenance record in `config/js-runtime-vendor.json` accounts for upstream build-path strings in one macOS libvips resource; it does not waive licence or credential checks. Chromium and guest Clash assets are separate resources. See the [component format and verification scope](docs/public/engineering/js-runtime.md).

## Guest Clash application resources

The guest application component uses the official Clash Meta for Android v2.11.36 ARM64 and x86_64 APKs from MetaCubeX/ClashMetaForAndroid, commit `043a6072225405b285a73e3d7b7e53431ccfc04c`. Its upstream GPL-3.0 text is preserved in [the vendor license](docs/public/vendor/clash-meta-license.txt); original third-party terms apply separately from AI Phone's Apache-2.0 source license. Pinned artifacts, maintainer verification and the pending corresponding-source/transitive review are described in [guest application resources](docs/public/engineering/guest-apps.md). The APKs are not part of a source export; binary promotion remains gated on complete distribution review.
