# Complete Windows factory

The Windows factory assembles one native x64 EXE containing the Android SDK/system
capsule and an application-owned Linux WSL backend. The Linux backend contains the
web interface, Rust services/OCR, media/channel services, database and desktop
images, engine, runtime tools, Android input/Home and guest Clash applications.
Users do not download these components or deploy a separate server.

Current source embeds the Evergreen WebView2 standalone installer using Tauri's
`offlineInstaller` mode. The native Android resource profile retains the x86_64
headless QEMU engine used by `-no-window`, its shared GPU/runtime libraries,
ADB, disk tools and upstream notices. It excludes other QEMU engines and the
standalone Qt interface from new Windows capsules. Existing SDKs and guest disks
are not edited. The packaged version command runs with `-no-window`; that check
does not establish a Windows guest boot.

Android and container-image compression now use the same Rust streaming XZ
encoder. x86 data uses BCJ filtering; ARM64 keeps its previous filter policy.
The envelope, CRC64, supported presets and 128 MiB decoder budget remain. The
encoder retains the existing free-space reserve and is a build tool, not an
Agent action check. No extra image-sized intermediate copy is added.

**Status:** the factory and native setup path are implemented. A successful full
factory receipt is required before a candidate is available. Native Windows
compilation and unpacked-payload inspection do not establish WSL2/WHPX/firewall
boot, installation or upgrade acceptance. Redistribution review and public binary
publication remain separate gates.

## Maintainer build

Run **Native client checks** with `complete_windows_factory=true`. The optional
`retain_complete_candidate` input keeps an inspected installer in an unpublished
draft. It does not publish a release.

1. A disposable Linux x64 builder creates the clean backend with both offline
   phone application groups and qualifies backend startup, restart, actual desktop
   frames and owned cleanup.
   The native client dependencies compile concurrently on a separate Windows
   runner. The exact target, release profile, Tauri custom-protocol feature and
   path-remapping flags are shared with installer compilation and native tests.
   Only this workflow run's artifact ID transfers the compiled target tree and
   Cargo registry; this does not reuse a previous source revision's build.
2. Only the sealed backend capsule and its source-bound metadata pass to the
   Windows job. The artifact ID and metadata digest come directly from the
   producing job; source commit, source manifest and capsule bytes must match.
3. The native Windows builder acquires the pinned stable Windows emulator, ADB and
   Android image, checks program versions, and builds a single EXE with both
   resource groups. The Linux server is not rebuilt as Windows services.
4. The inspector unpacks the EXE, checks the x64 executable, compiled resource
   identities, full resource inventory and private-material rules. A package over
   the single-file asset limit fails; delivery volumes are not accepted by this
   factory as the end-user artifact.

The older [ff69779 factory](https://github.com/Conferringo/AI-Phone/actions/runs/42324346884)
produced an inspected 2,111,739,124-byte EXE, retained unpublished. It predates the
headless profile, native compression and offline renderer. The Microsoft
standalone download was 214,927,568 bytes when inspected on 2026-11-07. A new full
build must prove the combined installer stays below the single-file limit; the
configuration and a small compression benchmark cannot prove that.

These are build-machine operations. Python/Node build tools are not an instruction
for an end user to install development environments. The Linux backend initializer
is Rust; remaining Python maintenance adapters and first-party Bun services still
await migration. This change does not establish the final Python-free architecture.

## Installed lifecycle

The configuration screen admits Windows hosting when both bundled Android and WSL
resource groups are present. Source-only and backend-only previews remain
incomplete. Runtime startup verifies the resource bytes, imports or resumes the
owned WSL backend, starts the independently owned native Android worker, verifies
the exact pair/input/notification channels, then creates or recovers the desktop.
Only a terminal scoped receipt completes the durable client checkpoint; late
cancellation cannot publish readiness. Restart revalidates live services instead
of treating an old checkpoint as a running device.

Windows must support WSL2 and WHPX. Automatic installation/enabling of missing OS
features and an offline WSL kernel package remain separate work; a complete
application payload does not prove these host prerequisites are already enabled.
Existing registered-backend upgrades also remain unfinished. User state stays in
the private installation directory rather than the program resource tree.

See [release status](release-status.md), [runtime format](runtime-bundles.md) and
[distribution requirements](preview-release.md).

## 中文

Windows工厂将原生Android系统/模拟器与独立WSL Linux后端组装进一个x64 EXE。
后端包含网页、服务、OCR、数据库/桌面镜像、运行工具、手机输入/桌面和Clash应用，
用户无需自行下载组件或部署服务。Linux工厂先验证启动、重启和真实桌面，再按同一
源码、artifact ID及摘要交给原生Windows构建和解包审查；依赖预编译与后端并行，
仅交接本轮同源、同目标及编译参数的产物，不放宽60分钟上限或减少完整检查。
旧ff69779整EXE已成功并保留未公开，但仍是在线WebView2引导器。新源码内置完整离线
渲染器，新Windows SDK只保留实际使用的x86_64无窗口引擎、共享库、ADB、磁盘工具和声明，
不修改已有SDK/设备盘。Android和镜像共用Rust流式XZ编码器，原格式与读取内存范围保持，
不增加用户操作关卡。当前源码的新整包容量、依赖与运行结果仍待验证。超过单文件上限直接失败，
不把分卷当作用户的最终下载包。

携带Android与WSL两类资源后才能进入Windows设备配置，真实配对、输入、通知和电脑
最终回执成功后才写完成检查点；旧检查点不等于设备正在运行。完整工厂结果、许可、
正式发布、Windows实机/WSL2/WHPX/防火墙仍分别验收。缺失系统功能的自动启用、离线
WSL内核、已注册后端升级与余下Python/Bun编排的Rust迁移尚未全部完成。
