# Native Android preparation

The Rust host runtime can admit an offline Android component and create one writable device configuration. The Windows native bootstrap calls this path when both the Android and WSL resource groups are bundled. Native startup, pairing and final device activation are wired separately; complete Windows distribution still requires qualification of acceleration, service discovery and fresh installation.

## Ownership and compatibility

The installer holds the existing installation lock for resource and device preparation. The component retains the factory's `ANDROID-MANIFEST.json` and content-addressed `components/android/<sha256>/sdk` layout. Supported component identities are macOS ARM64, Linux x64 and Windows x64 with their matching API 35 guest ABI. This does not qualify Linux ARM64 or Android as independent hosts.

The shared Rust archive reader handles source and Android capsules with separate fixed budgets. Source limits are unchanged. Android admission checks the compiled payload digest, compressed stream and tar completion, the exact inventory, every file digest and executable metadata, matching host/image and Windows filename collisions. SDK symlinks and Windows junctions are rejected. The component's existing `review-pending` metadata is preserved; validation does not grant redistribution permission.

The writable device lives under `user/data/android/avd`, separate from the immutable SDK. Its `ai_phone_p2.ini` record and `phone-local-01.avd` directory match the existing runtime. Resource choices are one, two or four CPUs, 1536–4096 MiB RAM from the existing presets, and 16, 24 or 32 GiB storage. A complete existing device is observed and reused without rewriting its configuration or data disk. A changed factory-image location requires a migration rather than an implicit replacement.

## Interruption and recovery

| State | Next invocation |
| --- | --- |
| No device or journal | Persist the selected resource intent, then create the device |
| Journal only | Continue the original intent |
| Journal and configuration directory | Verify its exact initial configuration before publishing the record |
| Journal and complete record | Verify the existing device and finish the journal |
| Complete device without a journal | Reuse its existing resources and disk |
| Partial device without a journal | Preserve it and report an unknown creation outcome |

The small protobuf creation journal covers the two-file publication boundary. It is removed after the configuration directory and record are durable. Cancellation leaves enough information to finish the original creation; a later request does not silently resize it. The code neither unregisters nor wipes a device. SDK extraction uses a temporary directory and publishes only a fully verified tree.

## Native component interface

`host-resources --prepare-android <installation>` reads a bounded `NativeAndroidPrepare` protobuf message from stdin until EOF and emits `NativeAndroidPrepared` on success. The bundled assistant dispatches the same command before model or database initialization. `--prepare-android-contract` reports `ai-phone-native-android-v1`. This is installer IPC, not a model tool or end-user setup command.

The receipt describes an SDK and device configuration. It contains no boot, root, frame, provisioning or complete-host success flag. Creating the actual 16–32 GiB data partition needs additional free capacity when the emulator first starts. First-run UI capacity checks and launch-time readiness remain distinct from resource preparation.

The macOS/Linux legacy installer retains its existing Python consumers. The native worker now owns SDK startup and optional WSL pairing; the client initializes its owned backend before asking that worker to boot or resume the phone. Paired transport evidence is separate from application/notification provisioning and complete two-device readiness. Windows WHPX/firewall and installed-host recovery still require qualification. See [device transport](device-bridge.md) and [release status](../release-status.md) for those boundaries.

## 中文

Rust 宿主组件已接通离线 Android 资源校验、展开与单台手机配置创建；Windows 原生初始化在同时携带 Android、WSL 资源时调用同一路径。系统镜像与可写设备盘分开存放，重复初始化保留原配置、账号和数据盘，不自动扩容或更换镜像。

目录、记录两处发布之间使用小型 protobuf 创建日志，取消后继续原资源意图；没有日志的未知半成品保持不动。配置创建、开机/root/画面、后端配对与完整部署使用不同回执。原生工作进程已接配对生命周期，但 Windows 加速/防火墙、完整设备初始化与实机恢复仍未验收；原 macOS/Linux Python 安装消费者也仍待迁移。
