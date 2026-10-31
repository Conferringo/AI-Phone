# Windows Android runtime component

The Windows resource factory builds the x64 emulator, platform tools and Android API 35 x86_64 image as one sealed component. This is a building block for the independent Windows client; it does not enable the unfinished Windows device installer.

The [factory policy](../../../config/android-factory.json) shares the guest image pin with Linux while keeping native SDK archives separate. Stable SDK selection requires Google's stable channel and license identifier. Archive size, vendor SHA-1 and independently computed SHA-256 are recorded. The [component producer](../../../scripts/android_component_factory.py) checks extracted SDK revisions against those pins before packaging. No existing AVD, account or guest disk is an input.

Admission uses `.exe` program names, canonical Windows filenames, case-insensitive collision checks and directory-junction rejection. Windows file permissions use the read-only attribute and the installer’s private-directory boundary rather than pretending POSIX execute bits are NTFS ACLs. Archive-body hashes remain mandatory on every platform. Unsupported host manifests and incomplete components cannot promote a client to independent deployment.

The [Windows workflow](../../../.github/workflows/windows-android-component.yml) builds from a fresh checkout, relocates the component, verifies its inventory and invokes the packaged emulator and ADB version commands under an isolated profile. Its receipt explicitly separates those checks from guest boot and WHPX/WSL lifecycle qualification. Only JSON and executable-line evidence are retained; SDK and system-image binaries remain on the disposable worker while redistribution review is pending.

## Maintainer build

Run on Windows x64 with Python 3.13. The work directory must be new. Device images are large; keep the component’s recovery reserve available.

```sh
python scripts/android_component_factory.py --work .tools/windows-android-component --verify-programs --receipt dist/windows-android-component.json
```

Full Windows delivery still requires the application-owned Linux/backend environment, Windows acceleration setup, device-control bridge, initialization and recovery. A successful SDK receipt alone does not qualify that delivery.
