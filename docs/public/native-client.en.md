# Native client development

[简体中文](native-client.zh-CN.md) · Development

## Development status

Complete product installers are not yet available. The distribution requires system images, emulator and prebuilt services; see [distribution requirements](preview-release.md). The source setup/connection implementation below is development code, not a complete product installer.

A reachable saved deployment opens directly. macOS can resume a completed installation owned by the same client payload; interrupted setup shows Retry. AI Phone → Setup (Command/Control+,) returns to setup or connection settings. Only a successful server address is saved. User files, credentials, devices and conversations are not replaced by a client installation.

The native setup and ten language catalogs share custom keyboard-accessible selectors. Errors and capacity checks stay visible during recovery. Stop cancels a stalled download and the install process group. Configure model opens the real workbench setup, not another disconnected native model form.

Only the native setup window has deployment IPC permissions. The remote workbench cannot download or run host processes. User state lives separately from versioned source; automatic upgrades and cross-version activation remain unfinished.

The native Rust Android worker can start an admitted SDK/AVD, verify root and a screenshot transport, and retain its phone after the setup pipe closes. A matching live session is observed and reused. Startup cancellation waits for the emulator console before sending one stop command; unknown cleanup stays an error. A stale owner with occupied ports requires recovery rather than duplicate startup. Windows setup calls this worker before WSL initialization, but pairing, guest provisioning and complete two-device readiness are still unfinished. Lifecycle fixtures and an isolated macOS Android boot are tested separately from Windows hardware acceleration and full-product installation.

The Rust guest phone peer receives pairing credentials and the emulator token through private protobuf stdin. It owns the existing media discovery file and local bridge listeners, preserves conflicting files, and removes its advertisement after draining connections. A Linux ARM64 peer has read the running macOS Android's root identity and real screenshot through the existing media client. Automatic Windows pairing, the bundled guest ADB/protocol tools and full guest provisioning remain to be integrated; complete-host readiness stays disabled.

## Build

```sh
./scripts/bootstrap.sh
cargo test --manifest-path apps/client/src-tauri/Cargo.toml --locked
bun scripts/build-client.ts --with-web-runtime
```

The packaging command refreshes a deterministic allowlisted source payload, verifies its hashes and includes license/notice resources. Use `--platform windows` for other clients on the appropriate build environment. The optional `--with-web-runtime` includes and cold-starts the matching prebuilt web component; other full-device components remain pending. See [runtime bundles](runtime-bundles.md). Source/native CI and expanded-payload inspection are build evidence, not physical installation acceptance.

macOS requires 14.4+ and Apple Silicon. Windows needs WebView2. macOS/Windows are unsigned. See [platform limits](preview-release.md), [third-party notices](../../THIRD_PARTY_NOTICES.md) and [release status](release-status.md).
