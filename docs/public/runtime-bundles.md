# Runtime bundles

AI Phone separates versioned application code, clean device baselines and persistent user data. The [Apple Silicon](engineering/macos-full-runtime.md) and [Windows](windows-client-factory.md) build guides define the seven resource groups required by a complete distribution. Published installers and platform qualification are listed in [release status](release-status.md).

## Package format

`scripts/prepare-client.ts` creates a deterministic `source.payload` archive and an integrity record. The client runs its native Rust initializer. Format 1 contains reviewed source. Format 2 declares a nonempty subset in this order: `web`, `native-services`, `phone-apps`, `container-host`, `guest-apps`. Source-only resources remain readable for contributors and stored-format compatibility.

| Component | Contents | Launch requirement |
|---|---|---|
| Web | Next.js standalone output, static/public assets, pinned host-matched Bun | Installed file hashes and contained dependency aliases must match |
| Native services | Release Rust assistant and phone runner, matched Rust PNG/OCR helper and generated fixture | Host, hashes, inventory and executable permissions must match; CPU Tesseract runs in the managed OCR service or the verified [Linux offline closure](engineering/linux-offline-ocr.md) |
| Phone applications | Unicode input/notification service and no-drawer launcher APKs | Source fingerprint, complete inventory and APK hashes must match; factory signatures/package IDs are verified |
| Container host | App-owned Lima, clean Alpine ISO and offline Docker/Compose packages; Linux also includes QEMU/SSH/firmware | Matching host runtime, ownership and persisted selection must match; [host component contract](container-host-runtime.md) |
| Guest applications | Pinned Clash Meta APK matching the guest ABI; existing dual-ABI components remain readable | Source pins, inventory and APK hashes must match; maintainer SDK inspection verifies signature, package/version and ABI |
| Android runtime | Matched emulator, platform tools and clean Android factory image | Host/guest ABI, complete file inventory and archive integrity must match; redistribution qualification is separate |
| Windows Linux guest | QEMU, WHPX, Linux kernel and QCOW2 base disk | Setup enables Windows Hypervisor Platform and creates a persistent overlay |
| Container images | Clean Linux desktop/workspace, PostgreSQL and Redis images, including desktop browser and Clash Verge Rev | Role, architecture, image identities and capsule integrity must match before import |


Each component manifest records file sizes, SHA-256 hashes and permissions. Native builds also record source fingerprints and compiler provenance. The packager accepts only matching builds. macOS helpers cannot require a newer OS than the client.

## Installation and startup

The native Rust initializer stages bundled source, Android and image resources directly. The Windows Linux guest uses the same native initializer. Guest task tools are independent of first-party runtime code.

The bundled [phone control applications](engineering/phone-apps.md) provide input, notification and launcher integration without first-run APK compilation. Supplying an Android runtime without these applications is rejected before device creation. Source contributor installations retain the compiler-based path.

The staging entry validates bounded archive headers and inventory before writing an immutable release directory. Duplicate entries, malformed paths, private paths, escaping aliases, unlisted files, changed bytes and occupied destinations are rejected. The version's `data` and `.env` link to the persistent `user` directory; staging a new version does not activate it or replace existing devices, credentials or conversations.

Service launchers verify installed native components before choosing their command. A present but invalid component fails explicitly. macOS and Linux use the prebuilt Rust binaries when supplied, and the runner receives the matching OCR path. Source-only installations retain contributor build commands. Messaging channels are included in the Rust assistant binary. Browser and desktop applications are supplied by the included desktop image; Android applications and the emulator/image have their own component inventories.


Image activation runs in Rust and keeps one open capsule through validation and import. Explicit OCI layouts retain their declared identities. Legacy Docker archives are streamed into an explicit OCI layout with content-addressed config/layer blobs and target names derived from the exact generated manifests. This avoids relying on a daemon-generated legacy manifest ID. The private `data/deployment/image-runtime.json` record retains the source archive metadata and records the selected targets; normalized selections carry `import_layout: "oci-v1"`. Old aliases, running containers and volumes are preserved. Factory/export tooling uses TypeScript and the native Rust encoder.

Each platform ships one complete installer under the GitHub asset limit.

## Component build and verification

These commands are for maintainers building components. Native OCR qualification requires a prepared backend. For a build-only host Tesseract, set `AI_PHONE_TEST_TESSERACT` to its executable and `AI_PHONE_TEST_TESSDATA` to the directory produced by `scripts/prepare-ocr-models.ts`. The pinned English and Simplified Chinese models work without Tesseract configuration files because the Rust helper selects TSV output directly. These qualification variables do not configure the installed client's OCR service.

```sh
./scripts/bootstrap.sh
./scripts/bun run build
./scripts/bun scripts/artifacts.ts native
./scripts/bun scripts/prepare-client.ts --with-web-runtime --with-service-runtime  --verify-web-runtime --verify-service-runtime
./scripts/bun scripts/build-client.ts --platform macos --with-web-runtime --with-service-runtime
```

For full distribution commands and factory-resource requirements, use the [Apple Silicon build guide](engineering/macos-full-runtime.md#build) or [Windows factory guide](windows-client-factory.md). The commands above exercise individual components.

Build on the matching host. Native services currently target macOS and Linux; Windows carries native Android and its matched QEMU Linux guest, including web and Rust services. Cross-compiling a client does not produce matching host services.

Verification stages the actual archive into an empty temporary installation. It launches the copied web runtime, checks `/phone`, JavaScript and PNG byte hashes, executes side-effect-free native version commands and recognizes a generated OCR image. An optional labelled, loopback-only disposable PostgreSQL fixture checks embedded migrations, the runtime role and DDL denial. No model credentials, user devices or production database are test inputs.

## Privacy and licensing

Next tracing excludes private state and environment files. The packager checks paths, credentials, escaping aliases and developer-home paths. Binary packages retain upstream component notices, terms and applicable corresponding source.

## Complete distribution requirements

| Component | Acceptance required |
|---|---|
| Android and emulator | Clean redistributable baseline, matching host ABI, offline device creation |
| Linux and container/VM runtime | Clean prepared image, offline import, owned process supervision |
| Media, channels and browser | Matching prebuilt dependencies and real startup checks |
| Windows host | Independent services and both guest devices |
| Updates | Signed artifacts, backup, migrations and rollback activation |
| First run | Client resource/model configuration followed by a usable workbench |

Physical installation on Apple Silicon macOS and Windows x64 remains unverified. See the [engineering records](engineering/index.md) for component checks and [release status](release-status.md) for package and redistribution status.

## 中文

客户端将版本化程序、干净设备基础和持久用户数据分开。网页组件包含 Next standalone、静态资源及对应宿主的 Bun；原生组件包含 Rust 助手、手机执行器与 OCR。macOS/Linux 启动已有原生组件时校验宿主、摘要和执行权限；组件损坏会明确失败，不回退到源码编译掩盖错误。

真实归档在空白目录验证网页资源、程序版本和 OCR。数据库验证只使用有专用标签且仅回环监听的临时实例，检查迁移、受限运行角色和 DDL 拒绝。手机与电脑共用 Rust PNG/OCR 流程；Tesseract 在隔离服务或 Linux 随包原生依赖中识别，Rust 负责画面摘要、文字边界和置信度解析。

macOS 安装包包含七组资源。Windows 安装包包含原生 Android 和 QEMU Linux 客体。首启、升级恢复和许可的核对结果见[发行状态](release-status.md)。
