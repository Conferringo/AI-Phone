# Guest application resources

The optional `guest-apps` component supplies Clash Meta for Android. Client packages include the APK matching their native guest: `arm64-v8a` on ARM64 hosts and `x86_64` on x64 hosts. The maintainer cache retains both pinned ABIs; an unused APK is omitted from the installation package. The selected desktop image already contains the Clash Verge Rev GUI; the installer does not acquire another DEB when an inventoried offline desktop image is selected.

Applications run inside the guest operating systems. Proxy subscriptions and application accounts are configured there and are not part of the distribution. An existing Android application install and its data are preserved.

## Desktop office applications

The desktop image includes LibreOffice Writer, Calc, Impress and Draw. The built-in `computer-use` Skill covers opening applications, Unicode input, spreadsheet formulas, presentation placeholders, shape dragging and saving the correct document format. Ordinary office tasks use the visible desktop and input tools. Command execution remains available for explicit programming or system-maintenance requests.

Keep source files and a local deliverable in the task directory. Check saved content in the application; naming a file `.xlsx` or `.pptx` does not select that format. Connected-device transfers carry file size and integrity receipts. Check delivery in the source conversation.

## Component contract

The source capsule contains `runtime/guest-apps/manifest.json` and the selected APK. The manifest lists exactly the included ABIs and files. Existing universal capsules containing both APKs remain readable. Selecting an ABI absent from an installed component reports the missing resource instead of downloading another APK. The outer host manifest inventories the component; its own manifest binds the producer, shared APK review policy and `config/clash-apps.json`. File sizes, SHA-256 hashes, package/version and signer identities come from the pinned source configuration, not editable artifact declarations. Changing both an APK and its artifact hash does not admit a different application.

Environment discovery selects `AI_PHONE_CLASH_ANDROID_APK` from the configured guest ABI. This path performs metadata checks without hashing the archives on every ADB command. Installation verifies the selected APK bytes against the source pin. A present but invalid component, missing selection or corrupt file fails explicitly; it never downloads a replacement. Source development without the component retains its separate pinned acquisition path.

`./scripts/bun scripts/artifacts.ts guest` is a maintainer operation after the two official APKs have been acquired into `.tools/clash-apps/android`. It checks size/hash, SDK signature output, package/version, native ABI and bounded unpacked privacy inspection, then atomically publishes `.tools/guest-apps`. It does not acquire subscriptions or install into an existing user phone. The maintained SDK inspection has macOS evidence; native Windows maintainer-tool execution is not yet qualified.

Use `--with-guest-apps` with `scripts/prepare-client.ts` or `scripts/build-client.ts` to include the verified component. Client initialization consumes the selected APK without Java, an SDK compiler or an APK download. [Runtime bundle contents](../runtime-bundles.md)

## Upstream and distribution review

Clash Meta for Android v2.11.36 uses the following upstream source revisions. The app's GPL-3.0 text is [included](../vendor/clash-meta-license.txt).

| Component | Revision | Source archive |
| --- | --- | --- |
| Android app and build scripts | `1a4f949213fcbb7f0c9e2a4a21138642d84b8f22` | [ClashMetaForAndroid](https://github.com/MetaCubeX/ClashMetaForAndroid/archive/1a4f949213fcbb7f0c9e2a4a21138642d84b8f22.tar.gz) |
| Embedded Mihomo core | `3f27ef39f8c96bbfbc8b0b9bb10f5c9ef60149d8` | [Mihomo](https://github.com/MetaCubeX/mihomo/archive/3f27ef39f8c96bbfbc8b0b9bb10f5c9ef60149d8.tar.gz) |

The [release run](https://github.com/MetaCubeX/ClashMetaForAndroid/actions/runs/39636620120) starts at `043a6072225405b285a73e3d7b7e53431ccfc04c`, updates the version to 2.11.36, and builds the commit listed above. Its [workflow](https://github.com/MetaCubeX/ClashMetaForAndroid/blob/043a6072225405b285a73e3d7b7e53431ccfc04c/.github/workflows/build-release.yaml) checks out submodules recursively and builds the core under `core/src/foss/golang/clash`.

The release used the [MetaCubeX Go 1.26.10 build](https://github.com/MetaCubeX/go/actions/runs/39623181948), based on [upstream Go 4e8c85f](https://github.com/golang/go/tree/4e8c85f5319d999bfd7617fdc191631e5634e3c5) and the [MetaCubeX patch](https://github.com/MetaCubeX/go/blob/91cc7b02d1f8667f469a9b1f03b0fc9fb6beca5c/go1.26.patch). The app tree applies two additional Android patches. The 120 certificates in both APKs reproduce byte-for-byte from [Ubuntu CA package 20260816ubuntu0.24.04.1](https://launchpad.net/ubuntu/+source/ca-certificates/20260816ubuntu0.24.04.1). Its Mozilla certificate data uses MPL-2.0. Go and Gradle dependency source review remains open.

Both pinned Clash Verge Rev 2.4.2 desktop packages contain two Mihomo builds. Embedded Go metadata identifies the [stable core at 84086a6](https://github.com/MetaCubeX/mihomo/tree/84086a6e6cc3aeb1c4cd04cddec497626d389c83) and the [alpha core at fed4b36](https://github.com/MetaCubeX/mihomo/tree/fed4b369a361d72be5549bca9042e42da3955a73). The architecture tags and dependency metadata identify [service helper source at 3961506](https://github.com/clash-verge-rev/clash-verge-service/tree/396150683e01e79740563561ae2fe2db28fb8904), whose Cargo manifest declares GPL-3.0. The bundled helpers use Rust 1.89.0. Historical build attestation and dependency notices remain open.

The pinned `Country.mmdb` and `geoip.dat` match [Loyalsoldier's 202509060052 release](https://github.com/Loyalsoldier/geoip/releases/tag/202509060052). Its [recipe](https://github.com/Loyalsoldier/geoip/blob/53bc174d523e64fac7e4b99f2af895720c9d6f94/config.json) uses MaxMind inputs. The pinned `geosite.dat` matches [MetaCubeX Git blob cb5fc1c](https://api.github.com/repos/MetaCubeX/meta-rules-dat/git/blobs/cb5fc1c68038f90568a71df9f67c27ad19627cef). Confirm the [MaxMind data terms](https://support.maxmind.com/knowledge-base/articles/who-is-covered-by-the-geolite-end-user-license-agreement) and exact upstream rule inputs before distribution.

## Availability

Current platform support, installer availability and distribution requirements are maintained in [release status](../release-status.md).
