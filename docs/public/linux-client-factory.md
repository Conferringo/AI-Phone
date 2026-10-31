# Complete Linux client factory

The maintainer factory assembles the Linux x64 client from clean, pinned inputs. Users configure their devices and model in the installed client; they do not build containers or obtain Android SDK archives themselves.

The [manual workflow](../../.github/workflows/linux-full-client.yml) uses a disposable Ubuntu 24.04 runner. It produces these resources before compiling the native Debian package:

- Standalone web, Rust services, channel/media services and their production dependencies.
- Phone control applications and architecture-matched Clash applications.
- Android API 35 x86_64 base image, emulator and platform tools.
- App-owned Lima, QEMU, firmware and offline container-engine packages.
- Clean workspace, Linux desktop, PostgreSQL and Redis images in one archive with shared layers.
- Portable Python/Node and the offline English/Chinese OCR runtime.

The [factory entry](../../scripts/linux_client_factory.py) uses fresh output directories and unique image references. It never exports a running container or reads guest disks, credentials or conversations. Android archives are fixed by size, Google-provided SHA-1 and the independently recorded SHA-256 in the [input inventory](../../config/android-factory.json). The [metadata selector](../../scripts/android_sdk_repository.py) requires the stable channel and its license identifier; archive ordering and numeric build IDs do not determine stability. Byte verification does not establish redistribution permission.

The compiled package is inspected against all nine required runtime components. Its receipt includes the source identity, artifact hash and actual component inventory. `complete_device_bundle` describes inspected contents; it does not mean that installation, graphics, audio or virtualization has been accepted on every Linux host. Linux graphics/audio libraries and KVM access remain host prerequisites covered by the installer preflight.

The workflow retains only a text qualification receipt. Installer publication remains subject to the [release policy](../../config/release-policy.json), redistribution review and corresponding-source requirements. Non-macOS physical installation is waived by the maintainer and must be disclosed as untested; compilation and payload inspection are still required.

## Local maintainer build

Use a fresh Linux x64 checkout with Docker Buildx with client/engine API 1.49 or newer and the containerd image store, Rust 1.96.0, the locked Bun dependencies, Java 17, Android build-tools 35 and the native Tauri compiler libraries. Keep at least 35 GiB free before starting.

```sh
./scripts/bootstrap.sh
python3 scripts/linux_client_factory.py
```

The factory writes the inspected Debian package and a JSON qualification receipt to `dist/`. These are build outputs, never an export of a user's installation.
