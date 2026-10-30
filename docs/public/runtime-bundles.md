# Runtime bundles

The client distribution contains versioned code and clean device baselines. User data belongs to a separate persistent directory. The current prebuilt **web component** is implemented; it is one component of the full distribution, not a complete offline installer.

## Package and launch contract

`scripts/prepare_client_host.py` produces the client resource `source.payload`, a deterministic gzip/tar archive, plus its standalone staging entry and integrity record. Format 1 contains reviewed source. Format 2 additionally declares the prebuilt web component. Source-only resources remain readable for development and compatibility.

The web component contains Next.js standalone server output, static/public assets and the pinned Bun executable for the build host. Its manifest records host architecture, file sizes, SHA-256 hashes, permissions and contained relative dependency aliases. It excludes source maps, tracing metadata and the unused native image optimizer. Normal media and attachment endpoints remain unchanged.

The staging entry checks the outer archive inventory before writing a new immutable release directory. It rejects duplicate entries, malformed paths, private data paths, oversized archives, unlisted files and hash mismatches. Dependency aliases are materialized only inside the web component. An occupied or changed destination is rejected. The release's `data` and `.env` link to the installation's persistent `user` directory; upgrades do not replace existing credentials or guest state.

The runtime verifies its host, installed bytes and dependency aliases before returning its launch command. macOS and Linux service launchers use that command when the web component is present, avoiding an end-user Next build. Other installation components still use the development bootstrap and are tracked separately below.

## Building and checking

These are **contributor commands**, not end-user setup steps:

```sh
./scripts/bootstrap.sh
./scripts/bun run build
python3 scripts/prepare_client_host.py --with-web-runtime --verify-web-runtime
python3 scripts/build_client.py --platform macos --with-web-runtime
```

Use the matching desktop platform for native packaging. The last command also builds and checks the web resource before packaging. Cross-compiling a client does not produce a runtime for the target host. Android needs its own Linux guest runtime; the builder rejects embedding a desktop-host web executable into the APK.

Verification stages the actual archive in an isolated temporary installation and launches its copied Bun. It supplies no model credentials, database configuration or device state. It requests `/phone`, compares every script response and a static PNG with installed bytes, and verifies that the retired image-optimization endpoint returns 404. It stops only the process it created. This proves relocated web startup and asset delivery, not device creation, model execution, audio or full first-run acceptance.

Desktop CI builds this web component on the matching Windows, macOS and Linux workers. Source CI also plants a synthetic private-path sentinel before the Next build, then packages and cold-starts the result. A complete distribution remains blocked until all required components and redistribution terms are verified.

## Privacy boundaries

Next file tracing excludes private `data`, `user` and environment files at the source. The independent packager rejects private paths, credentials, escaping aliases and local build-home paths. Generated server configuration is relocated only at known build-root fields. Third-party code is not rewritten to hide matches: generic upstream path examples are accepted only when bytes match the installed dependency and do not contain the builder's actual home path.

A local development build originally traced six private configuration/token files into its ignored standalone output. The packager rejected them before export or packaging. The corrected build was regenerated without those files. No token values or private output form part of public evidence.

## Remaining complete-distribution work

| Component | Current boundary |
|---|---|
| Web + Bun | Implemented prebuilt component; matching native CI is required for each change |
| Rust services, OCR and guest helpers | Still need matched prebuilt payloads and launch verification |
| Android system + emulator | Need clean redistributable baselines, matching host ABI and offline creation |
| Linux system + container/VM runtime | Need clean prepared images, offline import and owned process supervision |
| WhatsApp browser | Real QR bootstrap implemented; full distributions must include a reviewed matching browser |
| Windows and root/KVM Android hosts | Full two-device host integration remains incomplete |
| Notices and corresponding source | Source dependency metadata exists; full binary distributions require complete notices and applicable source offers |
| Updates | Versioned user-state separation exists; signed artifact delivery and full migration/rollback acceptance remain open |

macOS has a locally built client containing the web component. It has not been published as a complete installer. Non-macOS physical installation is waived by the maintainer; compilation, complete contents, privacy and licence checks remain required. Untested runtime behavior is disclosed, not marked passed. [Release gates](release-status.md) remain authoritative.

## Scoped simplification receipt — 2026-10-30

Coverage follows the native builder → archive producer → standalone staging entry → persistent links → service command → isolated HTTP check. This batch changes packaging, not business protobuf schemas, Redis behavior or database migrations. Paid APIs, guest creation and account-level message delivery are outside this check.

- **S1 — unused native image optimization, removed.** No web application consumer of `next/image`, Sharp or `/_next/image` was found. The separate device-stream service still uses Sharp and retains that dependency. Keeping their native binaries added host-specific packaging and licence obligations. Next now disables that endpoint and the web payload omits its dependency tree. Existing raw screenshots/attachments remain served normally. Checks compare a real PNG response with its installed hash and require optimizer 404. Re-enable the config and restore those dependencies if a future image consumer needs them; that introduces a new runtime/notice obligation. Upstream Next source is unchanged. Risk is limited to adding an optimizer consumer without updating the package contract.
- **S2 — duplicated path/integrity decisions, consolidated.** Archive staging owns canonical paths, contained aliases and installed-file verification; the web packager reuses them. The stored source-only format remains supported. Tests exposed and fixed acceptance of the directory path `.`, the macOS `/var` versus `/private/var` alias cycle check, and relative installation roots producing broken persistent links. Existing files and credentials are never overwritten. The cut removes parallel validators; it does not remove trust-boundary checks. Revert the staging/packager/consumer changes together if rollback is needed.
- **S3 — legacy release staging, retained.** `release_stage.py` serves the contributor source-release layout and its current-version pointer; the client uses hash-addressed resources. These are distinct persisted layouts with real consumers. Merging them without a migration would exchange a small module for compatibility machinery, so that candidate was rejected.
- **S4 — eager archive enumeration, removed.** `archive_records` previously loaded the complete member table before applying limits. `bounded_entries` now enforces the same count, per-file and total budgets while reading headers, before advancing into an oversized body. A truncated archive advertising a 201 MiB member reproduced the old late failure; it now receives the intended validation error immediately. Metadata-boundary checks cover zero/negative sizes, links, 20,000 entries and exactly 1 GiB. No stored format or valid archive capability was removed; rollback would restore the weaker late check and should not be used as a compatibility workaround. The cut replaces one eager traversal with a bounded traversal in the same owner, without introducing another parser or state store.


The four scoped Python modules have measured cyclomatic complexity at most 9 and CRAP below 10. Dryer found no candidates at similarity 0.8 in that scope. A targeted mutation run executed 53 mutations in seven path/integrity/header-budget functions and killed all 53; it did not mutate every function or prove whole-repository cleanliness. Separate tests cover corrupt/occupied archives, dependency resolution after removing the build tree, changed installed aliases and incorrect HTTP asset bytes. Private test logs and baseline failures stay local.

The first matching CI run exposed an OCR helper-start failure hidden behind a generic “not installed” error, and a Windows relocated module-resolution failure. OCR errors now retain the I/O kind; the EOF/filter regression uses an existing `cat` executable instead of generating a program and adds a real permission-denied check. Dependency aliases keep portable POSIX names in metadata and convert their targets to native paths when created. Windows/Linux CI now runs the relocated-package regressions before full packaging. The updated Source CI passed the OCR, payload and isolated PostgreSQL checks. The underlying first Linux spawn error cannot be recovered from the old generic message. A later Windows check exposed the isolation filter dropping uppercase `SYSTEMROOT`; the filter now matches variable names without case sensitivity while excluding model credentials. Python documents both [Windows environment uppercasing](https://docs.python.org/3/library/os.html#os.environ) and the [SystemRoot requirement for subprocesses](https://docs.python.org/3/library/subprocess.html#subprocess.Popen). Windows runtime acceptance still requires the corrected cold-start check to pass.

The repository-wide [SOLID audit](solid-audit.md), full runtime contents and platform acceptance remain open. No narrow packaging result promotes those broader gates.

---

## 中文

客户端把版本化代码、干净系统基础和用户数据分开。当前已实现预编译网页组件：Next standalone、静态资源、对应宿主的固定版本 Bun，以及文件摘要、权限和相对依赖链接。macOS/Linux 启动该组件时无需在用户端构建 Next。

打包、落盘和启动分别校验，私有路径在 Next 追踪源头排除，独立打包器再次拦截。资源展开到不可变版本目录，`data`、`.env` 指向持久 `user`；不替换现有设备、凭据和聊天。真实归档已在隔离目录冷启动，页面、13 项脚本与 PNG 内容校验通过，未使用模型密钥或现有设备状态。

这只是完整发行的一部分。Rust/OCR、Android/Linux 镜像、模拟器、容器/VM 运行时、浏览器、完整二进制声明及更新链仍须补齐。Windows 和 root/KVM Android 的双设备宿主管理尚未完成。桌面 CI 将构建匹配宿主的网页组件，Android 不混入桌面可执行文件。完整包门槛继续保持未通过，现有局部 DMG 不作为正式产品发布。

本批精简只涉及未使用的图片优化器及重复的路径/摘要决策；保留两个仍有消费者的发行目录格式。复杂度、重复度和 53 项定向变异检查只覆盖上述四个 Python 模块，不代表全仓已完全整洁。完整进度见[发行状态](release-status.md)。
