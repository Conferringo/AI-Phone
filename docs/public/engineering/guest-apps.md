# Guest application resources

The optional `guest-apps` component supplies Clash Meta for Android. Client packages include the APK matching their native guest: `arm64-v8a` on ARM64 hosts and `x86_64` on x64 hosts. The maintainer cache retains both pinned ABIs; an unused APK is omitted from the installation package. The selected desktop image already contains the Clash Party GUI; the installer does not acquire another DEB when an inventoried offline desktop image is selected.

Applications run inside the guest operating systems. Proxy subscriptions and application accounts are configured there and are not part of the distribution. An existing Android application install and its data are preserved.

## Desktop office applications

The desktop image includes LibreOffice Writer, Calc, Impress and Draw. The built-in `computer-use` Skill covers opening applications, Unicode input, spreadsheet formulas, presentation placeholders, shape dragging and saving the correct document format. Ordinary office tasks use the visible desktop and input tools. Command execution remains available for explicit programming or system-maintenance requests.

Keep source files and a local deliverable in the task directory. Check saved content in the application; naming a file `.xlsx` or `.pptx` does not select that format. Connected-device transfers carry file size and SHA-256 receipts. A verified phone copy does not establish that a social application delivered the file to its recipient.

## Component contract

The source capsule contains `runtime/guest-apps/manifest.json` and the selected APK. The manifest lists exactly the included ABIs and files. Existing universal capsules containing both APKs remain readable. Selecting an ABI absent from an installed component reports the missing resource instead of downloading another APK. The outer host manifest inventories the component; its own manifest binds the producer, shared APK review policy and `config/clash-apps.json`. File sizes, SHA-256 hashes, package/version and signer identities come from the pinned source configuration, not editable artifact declarations. Changing both an APK and its artifact hash does not admit a different application.

Environment discovery selects `AI_PHONE_CLASH_ANDROID_APK` from the configured guest ABI. This path performs metadata checks without hashing the archives on every ADB command. Installation verifies the selected APK bytes against the source pin. A present but invalid component, missing selection or corrupt file fails explicitly; it never downloads a replacement. Source development without the component retains its separate pinned acquisition path.

`./scripts/bun scripts/artifacts.ts guest` is a maintainer operation after the two official APKs have been acquired into `.tools/clash-apps/android`. It checks size/hash, SDK signature output, package/version, native ABI and bounded unpacked privacy inspection, then atomically publishes `.tools/guest-apps`. It does not acquire subscriptions or install into an existing user phone. The maintained SDK inspection has macOS evidence; native Windows maintainer-tool execution is not yet qualified.

Use `--with-guest-apps` with `scripts/prepare-client.ts` or `scripts/build-client.ts` to include the verified component. Client initialization consumes the selected APK without Java, an SDK compiler or an APK download. This is one component of the [complete bundle](../runtime-bundles.md), not proof that every platform's full installer is ready.

## Upstream and distribution review

Clash Meta for Android is pinned to [v2.11.36](https://github.com/MetaCubeX/ClashMetaForAndroid/releases/tag/v2.11.36), commit `043a6072225405b285a73e3d7b7e53431ccfc04c`. The upstream [license at that revision](https://github.com/MetaCubeX/ClashMetaForAndroid/blob/043a6072225405b285a73e3d7b7e53431ccfc04c/LICENSE) is preserved [locally](../vendor/clash-meta-license.txt). AI Phone's Apache-2.0 license does not relicense this third-party application.

Binary promotion remains subject to the existing redistribution gate. Corresponding-source, transitive native-component and complete-bundle delivery review remains pending; a pinned tag and a license copy alone do not clear that gate. Source-only commits do not include the APKs, guest profiles, subscriptions or user data.

## Availability

Current platform support, installer availability and distribution requirements are maintained in [release status](../release-status.md). Component formats and build interfaces do not independently qualify a complete installation.
