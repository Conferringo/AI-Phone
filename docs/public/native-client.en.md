# Native client preview

[简体中文](native-client.zh-CN.md) · 0.1.0-preview.2 · Experimental

## Download and use

Download your platform's installer from [Releases](https://github.com/Conferringo/AI-Phone/releases/tag/v0.1.0-preview.2). macOS Apple Silicon now includes [client-managed setup](integrated-hosts.md): choose language/device resources, install dependencies and devices, then configure your model. Fresh-host installation is untested. Windows, Linux and Android remain connection clients requiring an existing local/LAN deployment.

A reachable saved deployment opens directly. macOS can resume a completed installation owned by the same client payload; interrupted setup shows Retry. AI Phone → Setup (Command/Control+,) returns to setup or connection settings. Only a successful server address is saved. User files, credentials, devices and conversations are not replaced by a client installation.

The native setup and eight language catalogs share custom keyboard-accessible selectors. Errors and capacity checks stay visible during recovery. Stop cancels a stalled download and the install process group. Configure model opens the real workbench setup, not another disconnected native model form.

Only the native setup window has deployment IPC permissions. The remote workbench cannot download or run host processes. User state lives separately from versioned source; automatic upgrades and cross-version activation remain unfinished.

## Build

```sh
./scripts/bootstrap.sh
cargo test --manifest-path apps/client/src-tauri/Cargo.toml --locked
python3 scripts/build_client.py
```

The packaging command refreshes a deterministic allowlisted source payload, verifies its hashes and includes license/notice resources. Use `--platform windows|linux|android` for other clients on the appropriate build environment. Source/native CI and expanded-payload inspection are build evidence, not physical installation acceptance.

macOS requires 14.4+ and Apple Silicon. Linux packages target Ubuntu 24.04 compatible runtimes; Windows needs WebView2; Android targets ARM64. macOS/Windows are unsigned, and Android uses a retained preview signing identity. See [platform limits](preview-release.md), [third-party notices](../../THIRD_PARTY_NOTICES.md) and [release status](release-status.md).
