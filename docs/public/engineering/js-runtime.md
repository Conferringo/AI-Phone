# Offline JavaScript service component

This optional component supplies the media bridge, messaging channels and setup helpers without installing packages during initialization. It is a building block for the complete client, not a downloadable complete-device release. See [release status](../release-status.md).

## Ownership and format

`scripts/js_dependencies.py` resolves the installed, lockfile-controlled production graph for the three service applications. It keeps package versions, npm aliases, peer/optional declarations and host OS/CPU/libc selection. Workspace protocol files come from the frozen source snapshot. Package aliases retain the original resolution layout inside `runtime/js-services`; a flattened name-to-version map cannot represent all dependencies.

`scripts/prebuilt_js.py` compiles six entry points with pinned Bun: `media`, `channels`, `native-apps`, `device-network`, `messaging-browser` and `backend-qualification`. Compilation uses a bounded snapshot of reviewed sources and external packages. Source changes before publication invalidate the result. Package files, contained aliases, host identity, Bun version, source fingerprint and native minimum OS are recorded in the component manifest. Limits are 12,000 files and 256 MiB. Building and private-tool preparation preserve the existing 4 GiB recovery reserve.

The installer stages regular files before creating verified aliases. Reopening checks those aliases again. A present but corrupted component fails explicitly; source-only contributor installations retain their original path. Launch commands use the bundled Bun with `--no-install`. A private verified Bun copy also supports existing helpers whose imports use only built-in modules; unknown existing tools are preserved.

macOS launchd and the Linux supervisor consume the same role selector. This changes service commands when installing the component; it does not restart a contributor's running services or migrate devices/chats by itself. User data remains in the persistent installation directory.

## Maintainer build

These are component-production commands, not end-user setup requirements:

```sh
./scripts/bootstrap.sh
./scripts/bun run build
python3 scripts/prebuilt_js.py --build
python3 scripts/prepare_client_host.py --with-web-runtime --with-js-services
python3 scripts/build_client.py --platform macos --with-web-runtime --with-service-runtime --with-js-services
```

Build each dependency component on its matching host. Desktop CI builds this component on macOS ARM64, Windows x64 and Linux x64/ARM64; a configured CI step is not evidence that a particular run passed. Windows process ownership, ordinary-user aliases/ACLs and complete independent hosting still require their own acceptance.

## Availability

Current platform support, installer availability and distribution requirements are maintained in [release status](../release-status.md). Component formats and build interfaces do not independently qualify a complete installation.

## Provenance and remaining resources

Dependency resources retain their upstream licences and notices. One exact official libvips binary contains public upstream build-directory strings. `config/js-runtime-vendor.json` pins its package/version/path, full resource hash, registry archive URL, archive SHA-512 integrity and SHA-256. Only those exact bytes bypass the developer-path test; credential scanning, including UTF-16 forms, always applies. The installed bytes were compared with the official archive. This provenance approval is not redistribution clearance.

The Android media bridge still needs the matching emulator protocol files and a running authenticated emulator, provided by the separate Android runtime. The compiled browser/app setup helpers do not contain Chromium or the Clash application assets. Those resources, full native dependency notices/source obligations, complete assembly, fresh initialization and per-platform independent hosting remain release gates. The source component does not make first-run downloads acceptable for a complete product.

## 中文

本组件让媒体、渠道和初始化助手使用随包 Bun 与锁定生产依赖，初始化不再安装 JavaScript 包。源码快照、宿主、文件摘要、内部别名与原生最低系统版本共同确定组件身份；损坏明确失败，不回退到源码编译。已有设备、账号和聊天不进入组件。

媒体还需要匹配的 Android 协议资源和经过认证的设备。浏览器、客体应用、第三方声明与对应源码属于独立资源；当前安装包供应和各平台完整运行范围统一见[发行状态](../release-status.md)。

Device activation uses the host-matched native assistant binary with `--setup-devices`. Its read-only `--device-setup-contract` capability is qualified by the native builder and bound into the verified native component manifest. The installer reports readiness after the installation phases succeed and verifies the phone's root identity and both devices' frames. The JS services listed above and the Python installation stages are separate runtime dependencies.
