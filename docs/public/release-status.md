# Release status

Updated 2026-11-06. Source code and technical documentation are available. **Complete product installers are not published.** This page is the current reference for platform availability and known product limits.

## Platform qualification

| Target | Available implementation | Remaining delivery requirements |
| --- | --- | --- |
| macOS Apple Silicon | Native client and inspected complete App/DMG factory output | Fresh default-size dual-device installation, upgrades, signing/notarization and complete product acceptance |
| Windows x64 | Native Android/WSL activation and an inspected 2,118,848,707-byte single EXE from the [complete factory](windows-client-factory.md) | Candidate retention, WSL2/WHPX/network qualification, missing OS features/kernel, backend upgrade, complete first run and source/licence review |
| Linux x64 | Native client and inspected complete DEB/AppImage factory outputs | Single-file distribution, installed-client lifecycle and applicable source/licence review; physical installation remains untested |
| Linux ARM64 | Native client build | Matched Android/runtime resources and a complete independent host |
| Android ARM64 root/KVM | Native client and privileged-host target | Kernel/runtime integration, matched resources and complete dual-guest initialization |

The required experience is installation followed by device/model configuration and workbench use. Android and Linux baselines, emulators, services and execution runtimes belong in the distribution. The [Apple Silicon resource profile](engineering/macos-full-runtime.md), [Linux factory](linux-client-factory.md) and [runtime format](runtime-bundles.md) describe those package boundaries.

The complete [Mac factory](https://github.com/Conferringo/AI-Phone/actions/runs/41904124869) succeeded from `5ae36bc` with all nine runtime groups in one 2,084,332,533-byte DMG, below the single-asset limit. The earlier [Linux factory](https://github.com/Conferringo/AI-Phone/actions/runs/41758900254) succeeded from `e9ecab3` with complete DEB/AppImage outputs; its internal delivery volumes do not meet the single-download requirement. These are unpublished maintainer drafts from their stated source revisions. Successful factory output does not certify installation, source/licence closure or independent Windows/Android hosting.

The [Windows build from 2a5b17d](https://github.com/Conferringo/AI-Phone/actions/runs/42072341221) qualified backend startup/restart, assembled and inspected one complete EXE below 2 GiB, and passed 21 native client tests. Its final upload failed at a Windows shell line-ending boundary. The text receipt is retained; the installer itself is not available for download. The workflow therefore remains failed. Android guest boot and physical installation were not executed.

## Capability limits

- Real Android/Linux execution, input and persisted state exist in the development runtime. Component startup does not establish a complete first-time client installation.
- [Models and connections](model-and-channel-setup.md) include provider presets and protocol adapters. Support remains dependent on the configured model's capabilities and the application account; a preset is not certification of every provider or delivery path.
- [Copilot](copilot-voice.md) connects browser PCM capture, Rust/protobuf REST and persistent live-transcription adapters, durable recognized-text submission and streamed, interruptible WAV playback. Real microphone/provider quality, echo, end-to-end latency and guest microphone routing remain unverified. PCM16 output starts before synthesis completes; native speech-to-speech and guest calls remain unfinished.
- [Application guides](engineering/app-guides.md) bind qualified read-only Xiaohongshu and Writer methods to real observations. This does not qualify every social/work scenario or complete autonomous learning.
- Authenticated update discovery, complete activation/rollback and the full platform/language/media matrix remain in development. User data is kept separately from versioned program resources.

## Distribution requirements

`config/release-policy.json` owns publication gates. Binary publication remains disabled and `self_contained_device_bundle` remains false. Packages must satisfy full contents, source/privacy checks, third-party notices and applicable corresponding-source requirements before publication.

Non-macOS physical installation is waived for the current release scope and remains unverified. Compilation and component checks do not replace that result or qualify incomplete packages.

[Compatibility](compatibility.md) · [Distribution requirements](preview-release.md) · [Architecture](architecture.en.md) · [Contributing](../../CONTRIBUTING.md) · [Backup and recovery](backup-restore.en.md)

## 中文

源码与技术文档已公开，**完整产品安装包尚未发布**。macOS Apple Silicon 的5ae完整工厂已产出2084332533字节的单文件DMG，包含九类资源并低于附件上限；Linux x64早期完整DEB/AppImage工厂已成功，其内部分卷仍不等于用户单文件下载，均为未公开草稿。Windows 的2a5b17d已通过后端启动/重启、单文件EXE组装和检查及21项原生测试，包体2118848707字节，低于2GiB；最终上传因Windows命令输出换行问题失败，只有文本回执留存，安装包本体还不能下载。工作流整体仍记失败，Android客体启动与实机安装未测。WHPX/网络、缺失系统功能/内核、后端升级及首次就绪仍待验收。Linux ARM64与root/KVM Android仍缺匹配资源及完整双客体初始化。

Copilot 已接通浏览器 PCM 收音、Rust/protobuf 语音适配、连续文字提交与可打断播放；真实麦克风及服务商质量、回声、端到端速度和客体麦克风路由仍未验收。模型与连接应用按协议、模型能力及账号分别支持；配置预设不代表全部路径均已验收。

发行须满足完整组件、隐私、许可、对应源码及安装生命周期要求。非 macOS 实机安装按当前范围豁免并记为未测，不能据此把缺少组件或尚未接通的宿主标为完成。版本代码与设备、账号、聊天及用户文件保持分离。
