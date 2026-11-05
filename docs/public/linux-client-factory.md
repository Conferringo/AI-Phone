# Complete Linux client factory

The maintainer factory assembles the Linux x64 client from clean, pinned inputs. Users configure their devices and model in the installed client; they do not build containers or obtain Android SDK archives themselves.

The [manual workflow](../../.github/workflows/linux-full-client.yml) uses a disposable Ubuntu 24.04 runner. It produces these resources before one native compilation generates Debian and AppImage packages:

- Standalone web, Rust services, channel/media services and their production dependencies.
- Phone control applications and architecture-matched Clash applications.
- Android API 35 x86_64 base image, emulator and platform tools.
- App-owned Lima, QEMU, firmware and offline container-engine packages.
- Clean workspace, Linux desktop, PostgreSQL and Redis images in one archive with shared layers.
- Portable Python/Node and the offline English/Chinese OCR runtime.

The [factory entry](../../scripts/linux_client_factory.py) uses fresh output directories and unique image references. It never exports a running container or reads guest disks, credentials or conversations. Android archives are fixed by size, Google-provided SHA-1 and the independently recorded SHA-256 in the [input inventory](../../config/android-factory.json). The [metadata selector](../../scripts/android_sdk_repository.py) requires the stable channel and its license identifier; archive ordering and numeric build IDs do not determine stability. Byte verification does not establish redistribution permission.

The QEMU/OCR builders read the policy's fixed Ubuntu snapshot URI directly, using Noble main/universe and the existing Ubuntu archive signing key. Automatic current-repository snapshot discovery is disabled for this already pinned URI; no current-repository fallback or unsigned package source is introduced. OVMF shares QEMU's single index update. Both dependency archives must still match their original pinned hashes. [Ubuntu's snapshot documentation](https://snapshot.ubuntu.com/) describes the underlying APT source controls.

Each compiled package is unpacked and inspected against all nine required runtime components, source and licence files, and the native ELF architecture. AppImage inspection uses extraction mode without FUSE or starting the client. Its bundled graphics and media libraries have their own redistribution requirements. Each package has an independent source identity, artifact hash and component inventory in the factory receipt. `complete_device_bundle` describes inspected contents; it does not mean that installation, graphics, audio or virtualization has been accepted on every Linux host. Linux graphics/audio libraries and KVM access remain host prerequisites covered by the installer preflight.

The Android and container-image capsules use XZ preset9, with the existing128MiB decoder budget retained. This changes the build-time compression effort, not their system, image or file contents. AppImage's outer compression uses Zstandard. Its GUI libraries receive the normal AppImage relocation; the embedded host runtimes retain their verified bytes and original RPATHs through the [packaging adapter](../../scripts/linux-appimage-patchelf.sh). Post-packaging inspection requires those runtime bytes to match the sealed inputs.

The workflow retains a text qualification receipt. With `retain_complete_candidate` selected, both inspected packages and their checksums are retained in an unpublished maintainer draft. Both must be below the single-file delivery limit; an oversized DEB or AppImage fails the complete factory instead of being qualified through an internal volume handoff. Actual complete-package size still needs a successful factory result. Installer publication remains subject to the [release policy](../../config/release-policy.json), redistribution review and corresponding-source requirements. Non-macOS physical installation is waived by the maintainer and must be disclosed as untested; compilation and payload inspection are still required.

## Local maintainer build

Use a fresh Linux x64 checkout with Docker Buildx with client/engine API 1.49 or newer and the containerd image store, Rust 1.96.0, the locked Bun dependencies, Java 17, Android build-tools 35 and the native Tauri compiler libraries. Keep at least 35 GiB free before starting.

```sh
./scripts/bootstrap.sh
python3 scripts/linux_client_factory.py
```

The factory writes the inspected `.deb`, executable `.AppImage`, individual manifests and a combined JSON qualification receipt to `dist/`. These are build outputs, never an export of a user's installation.

## Availability

The package factory and installed-device lifecycle have separate qualification. Download availability, supported hosts and signing status are maintained in [release status](release-status.md). Build receipts and temporary diagnostic artifacts are not distributed as product packages.
