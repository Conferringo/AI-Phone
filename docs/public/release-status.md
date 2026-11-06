# Release status

Updated 2026-11-06. Source code and technical documentation are available. **Complete product installers are not published.** This page is the current reference for platform availability and known product limits.

## Platform qualification

| Target | Available implementation | Remaining delivery requirements |
| --- | --- | --- |
| macOS Apple Silicon | Inspected and retained complete single-file App/DMG factory output | Fresh default-size dual-device installation, upgrades, signing/notarization and complete product acceptance |
| Windows x64 | Inspected and retained complete single-file EXE; qualified backend startup/restart | WSL2/WHPX/network qualification, missing OS features/kernel, backend upgrade, complete first run and source/licence review |
| Linux x64 | Inspected and retained complete single-file DEB/AppImage outputs | Binary publication, installed-client lifecycle and applicable source/licence review; physical installation remains untested |
| Linux ARM64 | Native client build | Matched Android/runtime resources and a complete independent host |

The required experience is installation followed by device/model configuration and workbench use. Android and Linux baselines, emulators, services and execution runtimes belong in the distribution. The [Apple Silicon resource profile](engineering/macos-full-runtime.md), [Linux factory](linux-client-factory.md) and [runtime format](runtime-bundles.md) describe those package boundaries.

The complete [Mac factory](https://github.com/Conferringo/AI-Phone/actions/runs/42107153487) succeeded from `e5b9abd7` with all nine runtime groups in one 1,970,218,877-byte DMG. The complete [Linux factory](https://github.com/Conferringo/AI-Phone/actions/runs/42097421886) succeeded from `de79a091` with a 2,098,866,082-byte DEB and a 2,137,668,088-byte AppImage, each containing all nine resource groups. All three are single installer files below the asset limit; retained asset sizes and digests match their producer receipts.

The complete [Windows factory](https://github.com/Conferringo/AI-Phone/actions/runs/42123340112) succeeded from `6bd5a816`. It qualified Linux backend startup/restart, assembled and unpacked a 2,111,641,048-byte EXE with both Android and WSL resources, passed 21 native client tests, and retained the installer and its metadata in an unpublished draft. Dependency precompilation ran concurrently with backend qualification; the native job reused those outputs and retained its original 60-minute limit. This resolves the previous cold-compilation timeout and upload failure for that source revision.

These artifacts belong to their stated source revisions. They are unpublished maintainer candidates, not public downloads or installation certificates. Windows guest boot and physical installation were not executed. Package inspection does not close source/licence obligations.

## Capability limits

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

源码与技术文档已公开，**完整产品安装包尚未发布**。macOS Apple Silicon 的e5b完整工厂已留存1970218877字节单文件DMG；Linux x64 的de79完整工厂已留存2098866082字节DEB与2137668088字节AppImage，均含九类资源并低于2GiB，实际资产大小和摘要与生产回执匹配。

Windows的6bd完整工厂已成功：后端启动/重启、单EXE组装与解包检查、21项原生测试和最终留存均通过，EXE为2111641048字节，包含Android与WSL完整资源。依赖预编译与后端并行，安装器复用编译产物，原60分钟上限保持。此前冷编译超时和上传失败已在这一来源版本通过，但WHPX/网络、缺失系统功能/内核、后端升级、Android客体启动和实机首次就绪仍待验收。

这些是各自来源提交的未公开候选，不混用后续源码的资格。Linux ARM64亦未完成独立宿主。

Copilot 已接通浏览器 PCM 收音、Rust/protobuf 语音适配、连续文字提交与可打断播放；真实麦克风及服务商质量、回声、端到端速度和客体麦克风路由仍未验收。模型与连接应用按协议、模型能力及账号分别支持；配置预设不代表全部路径均已验收。

发行须满足完整组件、隐私、许可、对应源码及安装生命周期要求。桌面预览客户端的实机安装按当前范围豁免并记为未测，不能据此把缺少组件或尚未接通的宿主标为完成。版本代码与设备、账号、聊天及用户文件保持分离。
