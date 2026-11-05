# Complete Windows factory

The Windows factory assembles one native x64 EXE containing the Android SDK/system
capsule and an application-owned Linux WSL backend. The Linux backend contains the
web interface, Rust services/OCR, media/channel services, database and desktop
images, engine, runtime tools, Android input/Home and guest Clash applications.
Users do not download these components or deploy a separate server.

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

The [de79 factory](https://github.com/Conferringo/AI-Phone/actions/runs/42097398073)
qualified its backend, then exhausted the native job's 60-minute budget after
resource preparation and cold dependency compilation. Parallel dependency
compilation addresses that serial workload; the job limit, full-payload inspection
and native tests remain unchanged. Its complete installer result is still pending
validation on Windows.

These are build-machine operations. Python/Node build tools are not an instruction
for an end user to install development environments. The existing Linux backend
still contains legacy Python/Bun orchestration; its remaining Rust migration is
not concealed by the native client or this packaging path.

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
并行后的真实Windows整包结果仍待验证。超过单文件上限直接失败，
不把分卷当作用户的最终下载包。

携带Android与WSL两类资源后才能进入Windows设备配置，真实配对、输入、通知和电脑
最终回执成功后才写完成检查点；旧检查点不等于设备正在运行。完整工厂结果、许可、
正式发布、Windows实机/WSL2/WHPX/防火墙仍分别验收。缺失系统功能的自动启用、离线
WSL内核、已注册后端升级与余下Python/Bun编排的Rust迁移尚未全部完成。
