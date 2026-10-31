# Runtime bundles

AI Phone separates versioned application code, clean device baselines and persistent user data. Prebuilt web and native service components implement part of this distribution. Complete installers are not yet available; [release status](release-status.md) describes platform acceptance.

## Package format

`scripts/prepare_client_host.py` creates a deterministic `source.payload` archive, its standalone staging entry and an integrity record. Format 1 contains reviewed source. Format 2 declares a nonempty subset in this order: `web`, `native-services`, `phone-apps`, `container-host`, `js-services`, `guest-apps`. Source-only resources remain readable for contributors and stored-format compatibility.

| Component | Contents | Launch requirement |
|---|---|---|
| Web | Next.js standalone output, static/public assets, pinned host-matched Bun | Installed file hashes and contained dependency aliases must match |
| Native services | Release Rust assistant and phone runner, OCR helper, generated OCR fixture | Host, hashes, inventory and executable permissions must match |
| Installer tools | Portable Python and Node in a separate native resource group | Compiled manifest digest, host, complete inventory and immutable activation must match |
| Phone applications | Unicode input/notification service and no-drawer launcher APKs | Source fingerprint, complete inventory and APK hashes must match; factory signatures/package IDs are verified |
| Container host | App-owned Lima, clean Alpine ISO and offline Docker/Compose packages | Matching ARM64 macOS runtime, ownership and persisted selection must match |
| JavaScript services | Media, channels and setup helpers with the locked host-matched production package graph | Source fingerprint, complete hashes, contained aliases and native targets must match; launch with package installation disabled |
| Guest applications | Pinned Clash Meta APKs for ARM64 and x86_64 guests | Source pins, inventory and APK hashes must match; maintainer SDK inspection verifies signature, package/version and ABI |
| macOS OCR | Apple Vision helper with the client's minimum deployment target | macOS system frameworks |
| Linux OCR | Tesseract adapter | Python/Pillow, Tesseract and English/Chinese language data; these dependencies still need a complete offline payload |

Each component manifest records file sizes, SHA-256 hashes and permissions. Native builds also record source fingerprints and compiler provenance. The packager accepts only matching builds. macOS helpers cannot require a newer OS than the client.

## Installation and startup

The optional [portable installer tools](engineering/installer-tools.md) start the worker without a host interpreter. Native activation places verified tools in an immutable private component directory before source staging; service launchers retain those paths across restarts. Complete guest and service dependencies remain separate requirements.

The optional [phone control applications](engineering/phone-apps.md) remove first-run APK compilation. Supplying an Android runtime without these applications is rejected before device creation. Source contributor installations retain the compiler-based path.

The staging entry validates bounded archive headers and inventory before writing an immutable release directory. Duplicate entries, malformed paths, private paths, escaping aliases, unlisted files, changed bytes and occupied destinations are rejected. The version's `data` and `.env` link to the persistent `user` directory; staging a new version does not activate it or replace existing devices, credentials or conversations.

Service launchers verify installed native components before choosing their command. A present but invalid component fails explicitly. macOS and Linux use the prebuilt Rust binaries when supplied, and the runner receives the matching OCR path. Source-only installations retain contributor build commands. The optional [JavaScript service component](engineering/js-runtime.md) supplies media, channels and setup helpers without first-run package installation. Its browser and guest-application assets, guest images, emulator and complete platform integration remain separate requirements.

## Contributor build and verification

```sh
./scripts/bootstrap.sh
./scripts/bun run build
python3 scripts/build_native_services.py
python3 scripts/prebuilt_js.py --build
python3 scripts/prepare_client_host.py --with-web-runtime --with-service-runtime --with-js-services --verify-web-runtime --verify-service-runtime
python3 scripts/build_client.py --platform macos --with-web-runtime --with-service-runtime --with-js-services
```

Build on the matching host. Native services currently target macOS and Linux; Windows builds can carry the prebuilt web and JavaScript service components, while Android requires a separate privileged host runtime. Cross-compiling a client does not produce matching host services.

Verification stages the actual archive into an empty temporary installation. It launches the copied web runtime, checks `/phone`, JavaScript and PNG byte hashes, executes side-effect-free native version commands and recognizes a generated OCR image. An optional labelled, loopback-only disposable PostgreSQL fixture checks embedded migrations, the runtime role and DDL denial. No model credentials, user devices or production database are test inputs.

## Privacy and licensing

Next tracing excludes private state and environment files. The independent packager checks paths, credentials, escaping aliases and developer-home paths. Third-party dependency bytes retain their upstream identity and terms. Full binary packages require component notices and applicable corresponding-source delivery; the project's Apache-2.0 licence does not cover third-party redistribution permissions.

## Complete distribution requirements

| Component | Acceptance required |
|---|---|
| Android and emulator | Clean redistributable baseline, matching host ABI, offline device creation |
| Linux and container/VM runtime | Clean prepared image, offline import, owned process supervision |
| Media, channels and browser | Matching prebuilt dependencies and real startup checks |
| Windows and root/KVM Android hosts | Independent services and both guest devices |
| Updates | Signed artifacts, backup, migrations and rollback activation |
| First run | Client resource/model configuration followed by a usable workbench |

Non-macOS physical installation is waived for this release scope and remains unverified. Compilation, complete contents, privacy and redistribution checks remain required. Engineering records are available through [review evidence](engineering/index.md).

## 中文

客户端将版本化程序、干净设备基础和持久用户数据分开。网页组件包含 Next standalone、静态资源及对应宿主的 Bun；原生组件包含 Rust 助手、手机执行器与 OCR。macOS/Linux 启动已有原生组件时校验宿主、摘要和执行权限；组件损坏会明确失败，不回退到源码编译掩盖错误。

真实归档在空白目录验证网页资源、程序版本和 OCR。数据库验证只使用有专用标签且仅回环监听的临时实例，检查迁移、受限运行角色和 DDL 拒绝。Linux OCR 仍依赖系统 Python/Pillow、Tesseract 与语言数据，完整离线包须继续内置这些依赖。媒体/渠道离线组件另有真实归档运行证据，仍需完整镜像、模拟器、浏览器及其他宿主集成，完整安装包尚未提供。

The [linked-device browser runtime](engineering/channel-browser.md) shares one production selection policy across installation, binding and startup. New bundled accounts use the included image browser; existing host profiles retain their original runtime. Image import precedes browser verification. Complete bundle assembly and redistribution requirements remain part of the release gates.
