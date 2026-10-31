# Windows Android runtime component

The Windows resource factory builds the x64 emulator, platform tools and Android API 35 x86_64 image as one sealed component. It supplies the native Android resources for independent Windows hosting. WHPX/WSL integration, initialization and recovery have separate qualification.

The [factory policy](../../../config/android-factory.json) shares the guest image pin with Linux while keeping native SDK archives separate. Stable SDK selection requires Google's stable channel and license identifier. Archive size, vendor SHA-1 and independently computed SHA-256 are recorded. The [component producer](../../../scripts/android_component_factory.py) checks extracted SDK revisions against those pins before packaging. No existing AVD, account or guest disk is an input.

Admission uses `.exe` program names, canonical Windows filenames, case-insensitive collision checks and directory-junction rejection. Windows file permissions use the read-only attribute and the installer’s private-directory boundary. Archive-body hashes remain mandatory on every platform. Unsupported host manifests and incomplete components cannot promote a client to independent deployment.

The [Windows workflow](../../../.github/workflows/windows-android-component.yml) builds from a fresh checkout, relocates the component, verifies its inventory and invokes the packaged emulator and ADB version commands under an isolated profile. Its receipt explicitly separates those checks from guest boot and WHPX/WSL lifecycle qualification. Only JSON and executable-line evidence are retained; SDK and system-image binaries remain on the disposable worker while redistribution review is pending.

## Maintainer build

Run on Windows x64 with Python 3.13. The work directory must be new. Device images are large; keep the component’s recovery reserve available.

```sh
python scripts/android_component_factory.py --work .tools/windows-android-component --verify-programs --receipt dist/windows-android-component.json
```

Full Windows delivery still requires the application-owned Linux/backend environment, Windows acceleration setup, device-control bridge, initialization and recovery. A successful SDK receipt alone does not qualify that delivery.

## Qualification snapshot — 2026-10-31

[Windows build 41178729877](https://github.com/Conferringo/AI-Phone/actions/runs/41178729877) produced and relocated a 424-file component containing 2,916,859,281 uncompressed bytes. Its SHA-256 is `637f0a35e3510da8951b8a1f4a38ed166e296fd755d229c320de2899be48ec84`. The packaged emulator reported 37.2.13.0 and ADB reported 37.0.1. Native NTFS admission regressions passed. No Android guest was booted by this component check.
