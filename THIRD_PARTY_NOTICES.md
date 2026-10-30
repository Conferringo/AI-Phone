# Third-party sources and licence status

Original AI Phone code is licensed under **Apache-2.0**, selected by the maintainer. Third-party packages/assets retain their own licences. This document does not relicense dependencies. This experimental source preview excludes third-party binaries and guest images; binary redistribution review remains separate.

| Component | Source / licence evidence | Release action |
| --- | --- | --- |
| Runtime dependencies | Exact versions in Cargo.lock / bun.lock | [Inventory](docs/public/dependency-licenses.json): 572 Rust and 440 JavaScript packages across the server and native client, licence metadata and notice-file hashes. One legacy package (async 0.2.10) omits its SPDX field; its packaged MIT grant is retained and hashed. Redistribution/source-offer review remains open |
| Knowledge motion + native app icon | Original AI Phone TypeScript/SVG implementations, Apache-2.0; hashes in config/source-assets.json | The forest/welcome page has been removed. No EPIC renderer code is shipped. Complete knowledge-motion visual acceptance remains pending. |
| Android SDK/emulator/system image | Google's SDK packages | Download via sdkmanager after the user's licence acceptance; not bundled in the source archive |
| mise 2026.10.22 | [Official source / MIT](https://github.com/jdx/mise/tree/v2026.10.22) | Managed-host development downloads the pinned executable separately; not bundled in the client. Retain upstream notices in the installed tool distribution |
| Lima 2.2.1 and container-host image | [Lima source / Apache-2.0](https://github.com/lima-vm/lima/tree/v2.2.1), [Ubuntu cloud images](https://cloud-images.ubuntu.com/) | Downloaded separately for the optional private macOS container host; not bundled in clients/source. Guest OS packages retain their own licenses and source obligations |
| Clash Meta for Android 2.11.36 | [Official source / GPLv3](https://github.com/MetaCubeX/ClashMetaForAndroid/tree/v2.11.36); APK SHA256 and signature pin in config/clash-apps.json | Downloaded separately; not included in source archive. Preserve notices and corresponding-source obligations in binary distributions |
| Clash Party 2.0.4 | [Official source / GPLv3](https://github.com/mihomo-party-org/clash-party/tree/v2.0.4); deb pin in config/clash-apps.json | Unmodified package in Linux guest image; retain upstream licences and fulfil corresponding-source requirements before redistributing images |
| Extra Android apps | Separately installed APKs | No APK redistribution in source bundles; pin source, version, signature and licence before automating downloads |
| Docker images and guest packages | Pinned bases / Debian and package-manager dependencies | Preserve package notices and redistribution obligations in binary/image releases |
| Dunst and libnotify | [Dunst / BSD-3-Clause](https://github.com/dunst-project/dunst), [libnotify / LGPL-2.1-or-later](https://gitlab.gnome.org/GNOME/libnotify) | Installed by the guest package manager for desktop notifications; retain Debian copyright files and applicable redistribution obligations. Not bundled in preview.1 clients |
| Redis 8 Compose image | [Redis licence options](https://redis.io/legal/licenses/): AGPLv3, RSALv2 or SSPLv1, not the historic BSD terms | Optional service, not the Agent queue. Record the applicable option and exact image/source obligations before binary/image distribution; package-manager inventory does not cover this image |
| Docker Desktop prerequisite | [Docker Subscription Service Agreement overview](https://docs.docker.com/subscription-billing/desktop-license/) | Not bundled or relicensed by this project. User eligibility/acceptance is separate from Apache-2.0; the installer must not imply unrestricted free enterprise use |
| Mihomo v1.19.33 | [Corresponding source and GPLv3 licence](https://github.com/MetaCubeX/mihomo/tree/v1.19.33); hashes in config/mihomo-release.json | Installed from verified official releases; unmodified core, generated private configuration. Binaries are excluded from source archives. Binary/image releases must include required notices and corresponding-source access |
| GeoIP routing data | [MetaCubeX/meta-rules-dat](https://github.com/MetaCubeX/meta-rules-dat), immutable commit and hash in config/mihomo-release.json | Downloaded separately, excluded from source archives; retain upstream licence and data-source obligations in binary distribution |

The Apache-2.0 project licence does not override third-party, copyleft, trademark or asset obligations. SDKs, clients, guest images and installed packages are not bundled with the source archive. Binary/image redistribution needs its own notices and corresponding-source review. Generated assets and model names do not imply official affiliation.

GSAP 3.15.0 uses its own [Standard No Charge licence](https://gsap.com/community/standard-license/), not Apache/MIT or an OSI-approved project licence. The source depends on the separately installed package and preserves its notices. Its restrictions on competing visual animation builders remain applicable. Native libvips (LGPL), lightningcss/@vercel/og (MPL), caniuse-lite data (CC BY) and mixed-licence packages remain identified in the inventory.


The native client bundles collected licence texts in `docs/public/native-client-licenses.txt`. Complete text references and upstream grants for omitted crate notices are pinned in config/native-notice-supplements.json. The current client inventory reports no missing notice files. Source grants, full terms, copyright attributions and source availability remain distinct from a blanket legal certification.

Native-client notice supplements now cover 40 locked crates: exact VCS grants, verified published-archive grants, and full terms linked by upstream grants. Paths, source URLs, revisions/archive checksums and SHA256 are recorded in `config/native-notice-supplements.json`; no missing client notice files remain. The objc2 upstream grant/Apple-SDK caveat is preserved alongside the full MIT text and author attribution; this is not a claim to relicense Apple SDKs. SDKs, operating-system frameworks, WebView runtimes and guest applications are not redistributed in these client installers.

## Android Java/Kotlin runtime

`docs/public/android-client-licenses.txt` includes all 79 resolved runtime/BOM coordinates, Apache-2.0 terms and upstream embedded LICENSE/NOTICE attributions (including shaded FastDoubleParser notices). POM/notice files and hashes are in `config/android-dependency-notices.json` and `docs/public/licenses/android/`. Packaging compares the resolved release graph before accepting/signing the built artifact. Android system/WebView binaries are supplied by the device and are not included.

## Source availability and Linux system libraries

For MPL-2.0 client dependencies such as selectors/cssparser, exact crate versions and public source URLs are retained in the notice collection and Cargo.lock. selectors corresponding source is https://crates.io/api/v1/crates/selectors/0.36.1/download (checksum in the supplement record). The unmodified source can be downloaded and rebuilt; any in-repository native patch is separately preserved with its upstream license/source under apps/client/src-tauri/vendor. Linux installers dynamically use the distribution GTK/WebKit/system libraries rather than bundling their binaries; install those through the distribution package manager.

Preview clients include these texts. Unsigned clients and untested installations are explicitly labeled; no guest image/server binary is uploaded with them. Component terms and copyright notices continue to apply independently of Apache-2.0.

## Rust standard-library runtime

The clients are built with Rust 1.96.0. Its standard-library copyright collection and 13 accompanying licence/attribution files are pinned in config/rust-runtime-notices.json, verified against the installed official component manifest and supplied in the native notice collection. The full compiler is not bundled. Changing the preview toolchain requires a runtime-notice review.

### Location interface

The map uses Leaflet 1.9.4 under BSD-2-Clause. Its full [copyright and license](docs/public/licenses/javascript/leaflet/LICENSE) are retained. OpenStreetMap map data is attributed in the map and governed by [ODbL](https://www.openstreetmap.org/copyright); hosted tiles follow the provider's [tile usage policy](https://operations.osmfoundation.org/policies/tiles/). Tiles and the optional network-location lookup are contacted only after simulated location is enabled. No FakeLocation source or application binary is bundled.

## Local WhatsApp connection

Puppeteer is pinned to 25.16.0 via a root override; the older 24.38.0 transitive chain included unresolved extract-zip advisories. The replacement removes that chain and passed the real QR bootstrap test. This is our tested integration pin, not an upstream compatibility guarantee.

The optional local linked-device connection uses **whatsapp-web.js 1.34.7 (Apache-2.0)** and **qrcode 1.5.4 (MIT)**. Full upstream terms are retained in [whatsapp-web.js](docs/public/licenses/javascript/whatsapp-web.js/LICENSE) and [qrcode](docs/public/licenses/javascript/qrcode/license). Its legacy async 0.2.10 transitive dependency ships a [MIT grant](docs/public/licenses/javascript/async/LICENSE) without a package SPDX field. The dependency inventory records that missing metadata rather than inventing a declaration. WhatsApp and Meta do not sponsor this project; the Web protocol adapter is unofficial.

Chromium is a separate required runtime component for this optional connection. The developer bootstrap downloads a matching build into the ignored project tool directory; it is **not currently redistributed in a product installer**. Complete distributions must include its matching binary, full Chromium third-party notices/source obligations and provenance. The runtime-component publication gate remains closed. No personal browser profile, linked-device state or QR payload belongs in a source export.
