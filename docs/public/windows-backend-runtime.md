# Windows backend runtime

The Windows installer carries a Linux guest disk and kernel. QEMU runs the guest through Windows Hypervisor Platform. The factory builds the disk from the same sealed Linux root filesystem used by the WSL development and recovery tools.

## Contents

| Resource | Contract |
|---|---|
| Ubuntu base | Immutable base digest and retained distribution notices |
| Rust services | Assistant with channels, phone runner/media, OCR and native initializer |
| Web | Next.js standalone resources and matched Bun executable |
| Docker and Compose | Pinned Alpine package closure in an isolated musl prefix |
| Container images | Sealed workspace, desktop, PostgreSQL and Redis images |
| Phone bridge | Linux x64 ADB, upstream notices and the emulator media schema |
| User state | Created under `/var/lib/aiphone`; excluded from the distributed rootfs |

The TypeScript factory assembles a clean image with the source-bound Rust bootstrap. The phone bridge takes only ADB, version metadata and upstream notices from the Linux platform-tools archive. Complete Windows assembly includes the phone applications and guest Clash APK groups.

The Rust bootstrap lists managed containers together and checks existing containers in one inspection call. Full IDs, names and project ownership are verified before image activation.

The outer capsule contains `rootfs.tar.gz` and `WSL-BACKEND-MANIFEST.json`. Its manifest binds the compressed body, uncompressed inventory, required native entries, source/image identities and base license files. Rootfs inspection streams decompression and hashing together. Prepared native resources contain `rootfs.tar.gz` and `manifest.json`.

## WSL development and recovery

`aiphone-bootstrap --initialize-wsl-backend` reads configuration from stdin: locale, phone CPU/RAM/storage and desktop CPU/RAM. The configuration remains bounded to 8 KiB and uses the shared host-profile policy. Resume preserves profiles, passwords, tokens, devices and chats.

The installation is `/var/lib/aiphone/installation`; the owned engine socket is `unix:///run/aiphone/docker.sock`. Source staging, engine and image activation, database role preparation, migrations and service startup are Rust operations. Initialization imports bundled images without pulling or building them. It does not import a host Docker context or expose Windows drive mounts.

`backend_started: true` confirms the backend only. `phone_ready`, `complete_device_bundle` and `windows_boot_verified` remain false in that acknowledgement. The Windows native client subsequently boots or resumes Android and establishes the paired ADB/media connection. Device provisioning installs input/notification resources, creates or recovers the desktop, observes frames and emits `NativeWslDevicesReady` over protobuf. Only that receipt completes device setup.

Interrupted actions retain their installation/provisioning records. Recovery observes unknown outcomes rather than recreating a device or blindly repeating an action. Registered distributions retain their installed bootstrap; in-place backend upgrades are a separate lifecycle.

## Maintainer factory

```sh
./scripts/bootstrap.sh
./scripts/bun scripts/wsl-backend-qualification.ts --with-device-apps
```

Use an explicitly disposable GitHub-hosted Linux x64 workspace with `AI_PHONE_DISPOSABLE_QUALIFICATION=1`. The optional phone applications require Android 35 build tools and Java 17. The factory builds the same sealed source and images used by the backend, then imports the resulting rootfs into an owned, isolated container.

The existing Rust `release-tools wsl-qualify` worker runs as PID 1. It invokes the actual native initializer and verifies services, database roles, Redis, real desktop frames/media, work-file persistence and a database marker. Two phases reuse the same owned volume and verify restart persistence. The outer TypeScript driver records the worker receipts and removes only its labelled container, image and volume. Private logs, root filesystems and device/account data are not public evidence.

The qualification exercises the Linux backend. [Windows package and installation status](release-status.md)

WSL registration follows [Microsoft's import interface](https://learn.microsoft.com/en-us/windows/wsl/basic-commands#import-a-distribution) and [distribution VHD location](https://learn.microsoft.com/en-us/windows/wsl/disk-space#how-to-locate-the-vhdx-file-and-disk-path-for-your-linux-distribution).

## QEMU build inputs

On Windows x64, use an MSYS2 UCRT64 shell with native symlinks enabled. Install the UCRT64 compiler, Meson, Ninja, GLib, libslirp and zlib, then build the host component:

```sh
./scripts/bun scripts/windows-qemu.ts build/windows-qemu
```

The factory pins QEMU 11.1.3 and builds the WHPX system emulator and `qemu-img`. It records the PE import tree, DLL package versions, firmware, notices and source archive. This output still requires Windows qualification and redistribution review.

On a disposable Linux builder, build the pinned x86_64 kernel and convert a fresh sealed backend. Linux ARM64 builders use the `x86_64-linux-gnu-` cross toolchain.

```sh
./scripts/bun scripts/windows-guest-disk.ts kernel build/guest-kernel
sudo ./scripts/bun scripts/windows-guest-disk.ts \
  build/wsl-backend.payload "$BACKEND_SHA256" build/guest-kernel/bzImage \
  build/guest-kernel/modules.tar.gz build/guest-kernel/kernel-receipt.json \
  build/windows-guest.payload
```

The backend factory verifies the QEMU bootstrap command before sealing. The guest capsule contains its kernel, compressed QCOW2 disk, kernel config, license text and manifest. BusyBox starts the Rust bootstrap on the named virtio-serial port `org.aiphone.bootstrap`.

The Windows client starts an owned QEMU worker. It stores writes in a persistent QCOW2 overlay and opens the packaged base read-only. The worker passes a launch key through QEMU firmware configuration. The bootstrap and local control channels authenticate their protobuf messages with that key. The phone uses the existing ADB and emulator-media bridge. Setup waits for the observed phone pair and desktop receipt. Windows boot and restart qualification are pending.

## 中文

Windows 客户端用 QEMU 和 Windows Hypervisor Platform 启动内置 Linux 客体。客体包含 Rust 服务、网页、Docker/Compose、数据库和桌面镜像。用户数据保存在 QCOW2 写入层内。

原生 Android 通过 ADB 和媒体桥与 Linux 后端配对。双方返回 protobuf 回执后，客户端进入工作台。[发行状态](release-status.md)
