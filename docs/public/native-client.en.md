# Native client development

[简体中文](native-client.zh-CN.md)

The Tauri client manages local setup. Both platforms run native Android. macOS uses Lima for the Linux environment. Windows starts the bundled Linux guest with QEMU and Windows Hypervisor Platform. The workbench connects to the local services after initialization.

The setup screen supports ten languages, keyboard-accessible selectors, retry and cancellation. Return to setup through **AImanity → Setup**.

## Build

```sh
./scripts/bootstrap.sh
./scripts/bun scripts/build-client.ts --with-web-runtime
```

This command builds a development client. For complete packages, see the [macOS factory](engineering/macos-full-runtime.md) and [Windows factory](windows-client-factory.md).

macOS requires Apple Silicon and macOS 15.0 or later. The Windows installer targets Windows 11 x64. [Downloads and status](release-status.md)
