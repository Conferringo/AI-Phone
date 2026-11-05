# Release status

Updated 2026-11-06. Source code and technical documentation are available. **Complete product installers are not published.** This page is the current reference for platform availability and known product limits.

## Platform qualification

| Target | Available implementation | Remaining delivery requirements |
| --- | --- | --- |
| macOS Apple Silicon | Native client and inspected complete App/DMG factory output | Fresh default-size dual-device installation, upgrades, signing/notarization and complete product acceptance |
| Windows x64 | Native Android/WSL activation; qualified backend startup/restart | Complete native installer build/retention, WSL2/WHPX/network qualification, missing OS features/kernel, backend upgrade, complete first run and source/licence review |
| Linux x64 | Inspected and retained complete single-file DEB/AppImage outputs | Binary publication, installed-client lifecycle and applicable source/licence review; physical installation remains untested |
| Linux ARM64 | Native client build | Matched Android/runtime resources and a complete independent host |
| Android ARM64 root/KVM | Native client; [Rust root-host installation and Linux-guest worker](engineering/android-root-host.md) | APK/runtime integration, reviewed resources, real KVM qualification and complete GUI/dual-guest initialization |

The required experience is installation followed by device/model configuration and workbench use. Android and Linux baselines, emulators, services and execution runtimes belong in the distribution. The [Apple Silicon resource profile](engineering/macos-full-runtime.md), [Linux factory](linux-client-factory.md) and [runtime format](runtime-bundles.md) describe those package boundaries.

The complete [Mac factory](https://github.com/Conferringo/AI-Phone/actions/runs/41904124869) succeeded from `5ae36bc` with all nine runtime groups in one 2,084,332,533-byte DMG, below the single-asset limit. The complete [Linux factory](https://github.com/Conferringo/AI-Phone/actions/runs/42097421886) succeeded from `de79a091` with a 2,098,866,082-byte DEB and a 2,137,668,088-byte AppImage. Each Linux format is one installer file containing all nine resource groups; the retained assets match their producer inventories. These are unpublished maintainer drafts from their stated source revisions. Successful factory output does not certify installation, source/licence closure or independent Windows/Android hosting.

The latest [Windows factory](https://github.com/Conferringo/AI-Phone/actions/runs/42097398073), from `de79a091`, qualified backend startup/restart but reached its 60-minute native job limit while cold-compiling client dependencies. That job was cancelled and retained no new EXE. An earlier build produced an inspected 2,118,848,707-byte EXE, but its final upload failed and only its text receipt was retained. No complete Windows installer is available for download. Android guest boot and physical installation remain untested.

## Capability limits

- Real Android/Linux execution, input and persisted state exist in the development runtime. Component startup does not establish a complete first-time client installation.
- [Models and connections](model-and-channel-setup.md) include provider presets and protocol adapters. Support remains dependent on the configured model's capabilities and the application account; a preset is not certification of every provider or delivery path.
- [Copilot](copilot-voice.md) connects browser PCM capture, Rust/protobuf REST and persistent live-transcription adapters, durable recognized-text submission and streamed, interruptible WAV playback. Real microphone/provider quality, echo, end-to-end latency and guest microphone routing remain unverified. PCM16 output starts before synthesis completes; native speech-to-speech and guest calls remain unfinished.
- [Application guides](engineering/app-guides.md) bind qualified read-only Xiaohongshu and Writer methods to real observations. This does not qualify every social/work scenario or complete autonomous learning.
- Authenticated update discovery, complete activation/rollback and the full platform/language/media matrix remain in development. User data is kept separately from versioned program resources.

## Distribution requirements

`config/release-policy.json` owns publication gates. Binary publication remains disabled and `self_contained_device_bundle` remains false. Packages must satisfy full contents, source/privacy checks, third-party notices and applicable corresponding-source requirements before publication.

Physical installation is waived for previews on all four platforms and remains unverified. Compilation and component checks do not replace that result or qualify incomplete packages.

[Compatibility](compatibility.md) · [Distribution requirements](preview-release.md) · [Architecture](architecture.en.md) · [Contributing](../../CONTRIBUTING.md) · [Backup and recovery](backup-restore.en.md)

## 中文

源码与技术文档已公开，**完整产品安装包尚未发布**。macOS Apple Silicon 的5ae完整工厂已产出2084332533字节的单文件DMG。Linux x64 的de79完整工厂已成功，单文件DEB为2098866082字节、AppImage为2137668088字节，均含九类资源并低于2GiB；实际留存资源的大小与摘要和生产回执一致，仍是未公开草稿。

Windows最新de79工厂已通过后端启动/重启，原生依赖冷编译在60分钟的job上限被取消，未留新EXE。此前2118848707字节的完整EXE因上传失败也仅留下文本回执，目前没有可下载的完整Windows包。Android客体启动、WHPX/网络、后端升级及首次就绪仍待验收。root/KVM Android已有Rust安装和Linux客体执行单元，隔离测试覆盖即时重启、取消清理及恢复；仍缺APK接入、完整资源、真实KVM、GUI系统和双客体初始化。Linux ARM64亦未完成独立宿主。

Copilot 已接通浏览器 PCM 收音、Rust/protobuf 语音适配、连续文字提交与可打断播放；真实麦克风及服务商质量、回声、端到端速度和客体麦克风路由仍未验收。模型与连接应用按协议、模型能力及账号分别支持；配置预设不代表全部路径均已验收。

发行须满足完整组件、隐私、许可、对应源码及安装生命周期要求。四平台预览客户端的实机安装按当前范围豁免并记为未测，不能据此把缺少组件或尚未接通的宿主标为完成。版本代码与设备、账号、聊天及用户文件保持分离。
