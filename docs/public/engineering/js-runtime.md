# Offline JavaScript service component

This optional component supplies the media bridge, messaging channels and setup helpers without installing packages during initialization. It is a building block for the complete client, not a downloadable complete-device release. See [release status](../release-status.md).

## Ownership and format

`scripts/js_dependencies.py` resolves the installed, lockfile-controlled production graph for the three service applications. It keeps package versions, npm aliases, peer/optional declarations and host OS/CPU/libc selection. Workspace protocol files come from the frozen source snapshot. Package aliases retain the original resolution layout inside `runtime/js-services`; a flattened name-to-version map cannot represent all dependencies.

`scripts/prebuilt_js.py` compiles five entry points with pinned Bun: `media`, `channels`, `native-apps`, `messaging-browser` and `device-setup`. Compilation uses a bounded snapshot of reviewed sources and external packages. Source changes before publication invalidate the result. Package files, contained aliases, host identity, Bun version, source fingerprint and native minimum OS are recorded in the component manifest. Limits are 12,000 files and 256 MiB. Building and private-tool preparation preserve the existing 4 GiB recovery reserve.

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

Build each dependency component on its matching host. Desktop CI builds this component on macOS ARM64, Windows x64 and Linux x64/ARM64; a configured CI step is not evidence that a particular run passed. Windows process ownership, ordinary-user aliases/ACLs and complete independent hosting still require their own acceptance. Android root/KVM requires a separate host adapter.

## Verification

A macOS ARM64 combined archive has been unpacked outside the checkout using the actual portable installer Python. The OS sandbox denied reads throughout the original user home except for the immutable source capsule, and denied outbound WAN connections. Verification exercised:

- Package-independent web startup, 13 script resources and a static PNG.
- Bundled Bun execution with package installation disabled and no `NODE_PATH`.
- The existing built-in-only network helper's `prepare` operation against an isolated directory.
- Native Sharp/libvips image rotation, authenticated channel startup and protobuf media responses.
- One read-only screenshot of the already-running Android instance through the relocated media bridge. No input, restart, account binding or outgoing message was performed.
- Reopening the staged archive while retaining a private conversation fixture.

The tested component contains 7,081 files, 359 aliases and approximately 113 MB of data. Its two native macOS image libraries declare minimum macOS 11.0, within the client's 14.4 target. These results cover component execution, not a fresh 16 GB phone installation, real linked-account delivery, browser audio autoplay or physical speaker output.

## Provenance and remaining resources

Dependency resources retain their upstream licences and notices. One exact official libvips binary contains public upstream build-directory strings. `config/js-runtime-vendor.json` pins its package/version/path, full resource hash, registry archive URL, archive SHA-512 integrity and SHA-256. Only those exact bytes bypass the developer-path test; credential scanning, including UTF-16 forms, always applies. The installed bytes were compared with the official archive. This provenance approval is not redistribution clearance.

The Android media bridge still needs the matching emulator protocol files and a running authenticated emulator, provided by the separate Android runtime. The compiled browser/app setup helpers do not contain Chromium or the Clash application assets. Those resources, full native dependency notices/source obligations, complete assembly, fresh initialization and per-platform independent hosting remain release gates. The source component does not make first-run downloads acceptable for a complete product.

## 中文

本组件让媒体、渠道和初始化助手使用随包 Bun 与锁定生产依赖，初始化不再安装 JavaScript 包。源码快照、宿主、文件摘要、内部别名与原生最低系统版本共同确定组件身份；损坏明确失败，不回退到源码编译。已有设备、账号和聊天不进入组件。

macOS 真实组合归档已在禁止原用户目录读取、禁止外网的系统沙箱中展开，验证网页、原生图像处理、渠道认证、媒体 protobuf 和只读 Android 截图；重新展开保留私有聊天夹具。完整包仍需浏览器/应用资源、声明与对应源码、双设备完整初始化，以及其他宿主的独立运行能力，当前不发布部分安装包。
