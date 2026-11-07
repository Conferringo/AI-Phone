# Release status

Updated 2026-11-07. Source code and technical documentation are available. **Complete product installers are not published.** This page is the current reference for platform availability and known product limits.

## Platform qualification

| Target | Available implementation | Remaining delivery requirements |
| --- | --- | --- |
| macOS Apple Silicon | Inspected and retained complete single-file DMG candidate | Current-source bundle, integrated lifecycle, upgrades, signing/notarization and product acceptance; physical installation untested |
| Windows x64 | Inspected and retained complete single-file EXE candidate; backend startup/restart qualified | Offline WebView2, WSL2/WHPX/network qualification, missing OS features/kernel, backend upgrade and source/licence review |
| Linux x64 | Inspected and retained complete single-file DEB candidate; signed APT pipeline under qualification | Binary/channel publication, installed-client lifecycle and applicable source/licence review; physical installation untested |
| Linux ARM64 | Native client build | Matched Android/runtime resources and a complete independent host |

The required experience is installation followed by device/model configuration and workbench use. Android and Linux baselines, emulators, services and execution runtimes belong in the distribution. The [Apple Silicon resource profile](engineering/macos-full-runtime.md), [Linux factory](linux-client-factory.md) and [runtime format](runtime-bundles.md) describe those package boundaries.

Retained single-file candidate receipts: [Mac](https://github.com/Conferringo/AI-Phone/actions/runs/42107153487) `e5b9abd7`, 1,970,218,877-byte DMG; [Linux](https://github.com/Conferringo/AI-Phone/actions/runs/42097421886) `de79a091`, 2,098,866,082-byte DEB; [Windows](https://github.com/Conferringo/AI-Phone/actions/runs/42123340112) `6bd5a816`, 2,111,641,048-byte EXE. Contents and retained asset identities were inspected for those source revisions. They are unpublished maintainer candidates, not current-source downloads or installation certificates. Windows guest boot and physical installation were not executed.

## Rust migration

The final first-party runtime, installation/recovery, build, test and release tools must be Python-free; the Next.js frontend remains. Explicit database role preparation now runs in Rust/SQLx and is covered by isolated PostgreSQL tests with an empty PATH. Other host orchestration, recovery and build/CI paths still use Python, and channel/media JavaScript services also await their Rust migration. The current project does not yet meet this final architecture requirement.

Linux/WSL service start, resume, state inspection and stop now belong to the native Rust controller; existing installer imports retain only a temporary Python adapter. The APT index/sign/build tool and its real installation/upgrade test harness have also moved to Rust, and their former Python files are removed. These changes do not replace the remaining host initializers or make the retained historical packages current.

First-run profile creation, explicit configuration edits, schema/preset validation and initial database-password publication now also run in Rust. Owned container-engine state selection, restart environment and selected-engine startup are native too; their remaining installer imports are temporary adapters. Offline image admission, streaming import and activation records now use the native runtime. Legacy archives are normalized to explicit OCI targets while retaining source archive provenance and leaving old aliases untouched. Real Docker import/resume checks cover gzip, XZ and OCI; they do not constitute complete client installation. Full host orchestration, recovery and portable-tool retirement remain incomplete.

The read-only runtime environment policy is now native, including selected tool components, saved launch PATH, SDK/Java, guest resources, proxies and installed image/app metadata. It replaces the former Python policy and WSL's duplicate profile mapping. Client tool preparation shares that native tool implementation; the existing portable-tool and installer callers have not all been retired. This source change does not qualify new complete installers.

The macOS/Linux client entry now calls the Rust initialization controller; its former mise/Python runtime entry and download/layout modules have been retired. The controller connects the packaged resources, saved profile, owned engine, backend services, native phone, input/notification setup and native device completion. Desktop network directory preparation now calls shared Rust directly; the remaining legacy network caller invokes its installed/source Bun entry without a Python launcher and delegates directory changes to that same Rust owner. JavaScript network/channel/media services and other Python callers remain migration work. Local compilation and controller tests are not a completed cold install or a newly released package. WSL qualification currently reaches running services but fails during desktop provisioning; the underlying cause remains unconfirmed.

## Capability limits

The sealed WSL first-run initializer is now Rust, including its source/profile, engine, image, database and service sequence. Its former Python runtime orchestration has been removed, and the qualification driver calls the same native command. Factory verification, the qualification driver and portable Python used by other consumers remain; full first boot and newly assembled installers are not certified by the local component tests.

- Real Android/Linux execution, input and persisted state exist in the development runtime. Component startup does not establish a complete first-time client installation.
- [Models and connections](model-and-channel-setup.md) include provider presets and protocol adapters. Support remains dependent on the configured model's capabilities and the application account; a preset is not certification of every provider or delivery path.
- [Copilot](copilot-voice.md) connects browser PCM capture, Rust/protobuf REST and persistent live-transcription adapters, durable recognized-text submission and streamed, interruptible WAV playback. Real microphone/provider quality, echo, end-to-end latency and guest microphone routing remain unverified. PCM16 output starts before synthesis completes; native speech-to-speech and guest calls remain unfinished.
- [Application guides](engineering/app-guides.md) bind qualified read-only Xiaohongshu and Writer methods to real observations. This does not qualify every social/work scenario or complete autonomous learning.
- Authenticated update discovery, complete activation/rollback and the full platform/language/media matrix remain in development. User data is kept separately from versioned program resources.

## Distribution requirements

`config/release-policy.json` owns publication gates. Binary publication remains disabled and `self_contained_device_bundle` remains false. Packages must satisfy full contents, source/privacy checks, third-party notices and applicable corresponding-source requirements before publication.

Physical installation is waived for previews on Windows, Apple Silicon macOS and Linux and remains unverified. Compilation and component checks do not replace that result or qualify incomplete packages.

[Compatibility](compatibility.md) · [Distribution requirements](preview-release.md) · [Android component review](engineering/android-distribution.md) · [Architecture](architecture.en.md) · [Contributing](../../CONTRIBUTING.md) · [Backup and recovery](backup-restore.en.md)

## 中文

源码与技术文档已公开，**完整产品安装包尚未发布**。三桌面平台保留各自来源的完整单文件候选，见上表及构建回执；旧分卷安装器草稿已退役。Linux只提供DEB及复用该DEB的签名APT渠道，公开源尚未发布。Android客户端与AppImage已取消，桌面内的真实虚拟安卓手机仍保留。

最终自研运行链、安装/恢复与构建、测试、发行工具必须清除Python，前端保留Next.js。数据库角色初始化已迁到Rust/SQLx，并以空PATH执行隔离数据库验证；其余宿主编排、恢复和构建/CI仍有Python，渠道/媒体JavaScript服务也待迁Rust。当前不满足最终纯Rust目标，不能据此发布最终安装包。

Linux/WSL服务的启动、复用、状态和停止已迁至Rust，旧安装调用暂留薄Python适配。APT索引/签名/构建工具及真实安装升级测试也已迁至Rust，原两份Python文件已删除。这些改动不代表其余宿主初始化完成，也不把历史安装包视作当前源码版本。

首次配置、显式编辑、预设/结构校验、初始数据库密码发布及容器引擎状态管理已迁Rust。离线镜像准入、流式导入与激活记录也由Rust承担；gzip、XZ、OCI的真实Docker导入和恢复已验。密封WSL的源码/配置、引擎、镜像、数据库和服务初始化已串联到Rust，旧Python运行编排已删除，验收驱动调用同一原生命令。本地组件检查不代表完整首启或新安装包通过；工厂/验收工具、其他运行消费者、恢复及构建/发行工具的Python退役仍未完成。

只读运行环境策略也已迁Rust，包含工具组件、保存的PATH、SDK/Java、客体资源/代理及镜像/应用元数据，旧Python策略与WSL重复配置映射已替换；客户端工具准备复用同一原生实现。其余portable工具与安装调用尚未全部退役，此源码改动不等于新完整安装包验收。

macOS/Linux客户端入口已接入Rust初始化控制器，原mise/Python入口及下载/布局模块已退役；控制器串联包内资源、保存配置、自有引擎、后端服务、原生手机、输入/通知与设备完成检查。桌面网络目录准备现直接调用共享Rust；其余旧网络调用直接启动对应包内/源码Bun入口，目录变更也交给同一Rust实现。JavaScript网络/渠道/媒体服务与其他Python调用仍待迁移。本地编译/控制器测试不等于完整冷安装或新包发布。WSL当前实际验收到服务运行，但桌面创建失败，底层原因仍未确认。

这些是各自来源提交的未公开候选，不混用后续源码的资格。Linux ARM64亦未完成独立宿主。

Copilot 已接通浏览器 PCM 收音、Rust/protobuf 语音适配、连续文字提交与可打断播放；真实麦克风及服务商质量、回声、端到端速度和客体麦克风路由仍未验收。模型与连接应用按协议、模型能力及账号分别支持；配置预设不代表全部路径均已验收。

发行须满足完整组件、隐私、许可、对应源码及安装生命周期要求。桌面预览客户端的实机安装按当前范围豁免并记为未测，不能据此把缺少组件或尚未接通的宿主标为完成。版本代码与设备、账号、聊天及用户文件保持分离。
