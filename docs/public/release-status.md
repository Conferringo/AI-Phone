# Release status / 发行状态

2026-10-29 在线源码：[Conferringo/AI-Phone](https://github.com/Conferringo/AI-Phone)，实验性技术预览；[CI实际状态](https://github.com/Conferringo/AI-Phone/actions)。私密漏洞报告已启用，无安装包Release。

2026-10-29 最新决定：[CSS与源码预览审查](css-review-2026-10-29.md)。维护者授权实验性源码公开，不再等待独立设备安装测试，未实测平台留给社区 Issue 反馈。`clean_host_install` 仍为 false，并在策略中记录豁免，不能写成通过。完整产品仍有 UI/平台、许可及长时恢复缺口；本次只提交白名单源码与文档，不上传安装包或用户状态。

2026-10-29：[本体收敛与安装语言](core-retirement-2026-10-29.md)：桌宠从本体退役，素材及浏览器旧偏好清理，后端/数据库无专属对象；安装第一项选语言，原生到工作台偏好传递；生产下拉统一自绘与控件高度。以下为当批历史记录；最新状态以上方记录为准。

2026-10-29：[首次配置与窗口审查](release-ux-audit-2026-10-29.md)新增现有配置中心内的引导、明确模型协议/预设、实际设备画面检查，并修复表单提交和渠道轮询问题。真实 DeepSeek 三项能力检查及双设备画面通过；八语言原生连接窗已在重建后的 macOS ARM64 客户端验收，其他平台新界面仍待验收。4 道产品门槛仍开放，详见[中文路线](release-roadmap.zh-CN.md) / [English roadmap](release-roadmap.en.md)。

2026-10-29：[声音与稳定性复查](release-stability-2026-10-29.md)已修复客体音频守护、采集进程回收、播放器重连和焦点生命周期；Skill未知提交与共享PG恢复故障回归通过，严格Clippy通过。完整隔离恢复CI已复跑。物理扬声器听感、全新主机及完整平台验收仍未关闭，发行门槛保持真实状态。

2026-10-28 · **0.1.0-preview.1 internal candidate**

Full product acceptance is defined in the [release contract / 发行交付标准](release-contract.zh-CN.md) ([English](release-contract.en.md)). A local macOS ARM64 native-client DMG now exists; see the [client preview](native-client.en.md). It requires an existing deployment and has no Developer ID signing/notarization. Windows x64 NSIS, Linux ARM64 DEB and Android ARM64 APK have now been built locally; their distinct runtime limits are in the [platform evidence](multiplatform-clients.md). A full server installer is separate. [Privacy](../../PRIVACY.md) and [disclaimer](../../DISCLAIMER.md) are engineering drafts; per-artifact legal/distribution review remains open.

## 已实现并有检查 / Implemented and checked

- Existing real Android/Linux local workbench, persistent chat, Agent registry, grounded actions and device file exchange; see architecture and explicit task/platform limits.
- Guided CLI install/configure/doctor/start/stop/open; source packaging with exclusions, manifest and SHA256.
- Managed version staging with independent user storage. Isolated tests preserve sample user files/.env, avoid activating an update, reject tampered files, credentials and symlinks.
- Responses + new Chat Completions translation, tested through a real local HTTP mock endpoint; text/image/tool mapping and incomplete-stream rejection.
- Telegram adapter code and isolated tests; existing QQ/Feishu/WeCom adapters retained.
- Native Clash apps preinstalled in Android/Linux, guest-owned VPN/TUN, no web network page and direct host defaults; native subscription handling, real downloads, synthetic notification wake and guest restart checks passed.
- Eight UI catalogs (simplified/traditional Chinese, English, Japanese, Korean, German, French, Spanish), localized shared windows, direct personal-workbench startup and separated layout/camera motion; two sanitized built-in app Skills distilled from previously audited real Android experience.

These checks do not establish a clean-machine installation, a real Telegram account, all language screens or every native platform. The source installer has prerequisites. Current doctor may correctly refuse a fresh install when disk space is insufficient.

## 技术预览公开前 / Before public technical preview

| Gate | Acceptance |
| --- | --- |
| Clean host installation | Pending: a separate Mac installs prerequisites/source, creates default devices, configures an Agent, performs file exchange and survives an interrupted install. Same-host independent source builds and empty DB/Skill initialization passed. |
| Backup/restore | Passed: consistent private full-state backup; empty-target transactional restore; 33 table fingerprints, 3,019 state files; offline restored Android boot and Linux GUI/root/work files. Automatic activation/update orchestration remains future work. |
| UI acceptance | Actual pages: English/Chinese input/save/errors, all device layouts, narrow windows, reverse/reduced motion; native Clash and direct native-client startup checked; complete language/backend-error, accessibility and workbench/window matrix remains pending |
| Licences/provenance | Original code Apache-2.0 selected; original knowledge motion/app icon implemented, desktop companion removed from the core; extension design documented separately. Final asset/dependency/binary redistribution review and visual acceptance remain open. |
| Durable action retention | Passed in this batch: sharded durable records, 128-entry memory cache, old/unknown result retention, single-runner lock, migration and live cold replay; backup/restore across hosts remains part of the clean-host gate |
| Database runtime role | Passed: production uses a DML-only runtime login; explicit migrations use a separate owner login. Bootstrap administrator preserved; empty/legacy/rollback/denied privilege tests passed. |
| Long-running recovery | Worker fault matrix passed: SIGKILL, PG stop/start, process suspension, original waits, unknown-result refusal, provider backoff/quota, incomplete SSE and 56 real tool turns. Actual host sleep/reboot/power loss and prolonged multi-device soak remain open. |

[前端/后端/数据库/protobuf/Redis专项审计 / Release audit](release-audit-2026-10-28.md) and [本批前后端审计 / Current code audit](code-audit.md) records the fixed issues and its scope.

`config/release-policy.json` records these gates. `./ai-phone package --public` refuses while any gate is open. A successful internal package is not permission to publish.

## 后续发行工作 / Subsequent product work

Signed update/backup/migration/rollback lifecycle; complete translation and error codes; Linux/Windows server adapters; remaining platform signing/install/runtime tests. Android initially connects to the user's server; an ordinary Android phone hosting the entire service is not promised.

An exact completion percentage/date would currently be misleading: platform installation and long-duration behavior have not been measured on clean machines. The next concrete milestone is a reproducible macOS source preview with real backup/restore, then Linux server portability and signed clients. Public documentation should state support actually verified rather than a planned platform list.

[Database roles](database-roles.md) · [Backup/recovery](backup-restore.en.md) · [备份恢复](backup-restore.zh-CN.md) · [Dependency licence inventory](dependency-licenses.json). Reproduce source installation preparation with `python3 scripts/verify_source_install.py --output /private/new-check --pg-container aiphone-disposable-pg --pg-port 55499`; the test container must have label `dev.aiphone.test=permissions`. It never starts production jobs or devices.

[Task recovery and installation retry evidence](task-recovery.md): atomic task finalization and database-outage persistence retry are deployed. Installation phases now have private durable checkpoints and artifact/source validation; start resumes missing services without restarting running tasks. This does not clear the clean physical-host gate.

Source handoff: [English](source-release.en.md) / [中文](source-release.zh-CN.md). Earlier batches were local-only; the latest publication decision is recorded at the top of this document.

## 追加客户端发行审计 / Native-client release audit

[2026-10-28 audit](client-release-audit-2026-10-28.md) covers native origin switching/IPC/startup, bounded HTTP bodies, real knowledge-label layout and dependency advisories. The local macOS app/DMG now has a file-level privacy/resource manifest; a separate native CI workflow is prepared but has not run remotely. The Linux GLib issue now has an exact upstream backport with full-file verification and optimized iterator regressions; its original advisory stays visible in the [audit report](dependency-security.json). Upstream-pinned notice supplements close 14 missing crate archives; 26 complete-notice gaps remain. These changes do not clear the full product/clean-host/signing gates.
