# Guest application resources

The optional `guest-apps` component supplies Clash Meta for Android for both `arm64-v8a` and `x86_64` guests. It is independent of the client host architecture. The selected desktop image already contains the Clash Party GUI; the installer does not acquire another DEB when an inventoried offline desktop image is selected.

Applications run inside the guest operating systems. Proxy subscriptions and application accounts are configured there and are not part of the distribution. An existing Android application install and its data are preserved.

## Component contract

The source capsule contains `runtime/guest-apps/manifest.json` and two APKs. The outer host manifest inventories the component; its own manifest binds the producer, shared APK review policy and `config/clash-apps.json`. File sizes, SHA-256 hashes, package/version and signer identities come from the pinned source configuration, not editable artifact declarations. Changing both an APK and its artifact hash does not admit a different application.

Environment discovery selects `AI_PHONE_CLASH_ANDROID_APK` from the configured guest ABI. This path performs metadata checks without hashing the archives on every ADB command. Installation verifies the selected APK bytes against the source pin. A present but invalid component, missing selection or corrupt file fails explicitly; it never downloads a replacement. Source development without the component retains its separate pinned acquisition path.

`scripts/prebuilt_guest_apps.py --build --tools SDK/build-tools/35.0.0` is a maintainer operation after the two official APKs have been acquired into `.tools/clash-apps/android`. It checks size/hash, SDK signature output, package/version, native ABI and bounded unpacked privacy inspection, then atomically publishes `.tools/guest-apps`. It does not acquire subscriptions or install into an existing user phone. The maintained SDK inspection has macOS evidence; native Windows maintainer-tool execution is not yet qualified.

Use `--with-guest-apps` with `scripts/prepare_client_host.py` or `scripts/build_client.py` to include the verified component. Client initialization consumes the APKs without Java, an SDK compiler or an APK download. This is one component of the [complete bundle](../runtime-bundles.md), not proof that every platform's full installer is ready.

## Upstream and distribution review

Clash Meta for Android is pinned to [v2.11.36](https://github.com/MetaCubeX/ClashMetaForAndroid/releases/tag/v2.11.36), commit `043a6072225405b285a73e3d7b7e53431ccfc04c`. The upstream [license at that revision](https://github.com/MetaCubeX/ClashMetaForAndroid/blob/043a6072225405b285a73e3d7b7e53431ccfc04c/LICENSE) is preserved [locally](../vendor/clash-meta-license.txt). AI Phone's Apache-2.0 license does not relicense this third-party application.

Binary promotion remains subject to the existing redistribution gate. Corresponding-source, transitive native-component and complete-bundle delivery review remains pending; a pinned tag and a license copy alone do not clear that gate. Source-only commits do not include the APKs, guest profiles, subscriptions or user data.

## Verification scope

Local tests exercise both ABIs, exact source pins, rewritten artifact hashes, alias rejection, missing/corrupt files, failed publication, source changes during inspection, runtime profile selection and preservation of existing fixture data. The actual official APKs have passed the maintained SDK/signature/package/ABI and privacy checks.

On macOS, the bundled Python staged the actual `js-services` + `guest-apps` source capsule inside an OS sandbox denying all network, original home-directory source and SDK access. The compiled application consumer verified both official APKs without creating a source download cache. A corrupted APK failed, and repeated staging retained a chat fixture. That diagnostic capsule did not include full guest images or create a new phone. Fresh-device installation and four-platform complete-host acceptance remain separate requirements.
