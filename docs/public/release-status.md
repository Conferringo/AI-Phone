# Release status / 发行状态

Updated 2026-10-29. **Public experimental preview; not a finished four-platform product. / 已公开实验性预览，尚非成熟的四平台产品。**

## Published preview / 当前下载

- [v0.1.0-preview.2](https://github.com/Conferringo/AI-Phone/releases/tag/v0.1.0-preview.2) introduces experimental client-managed installation on macOS Apple Silicon: device resources, owned tools, Android licenses, service preparation, desktop provisioning, install recovery and model configuration handoff. **Full fresh-host dual-device installation remains untested.**
- Windows x64 NSIS, Linux amd64/ARM64 DEB and Android ARM64 APK remain connection clients. Their independent host adapters are unfinished. macOS/Windows are unsigned; Android uses a preview certificate. Physical installation acceptance remains waived and untested.
- [preview.1](https://github.com/Conferringo/AI-Phone/releases/tag/v0.1.0-preview.1) is retained unchanged as the earlier connection-client release. New source does not rewrite its tags or installer bytes.
- There is no stable release. `/releases/latest` excludes prereleases; use the explicit tag. Artifact manifests record build source, hashes, notices and limitations.

## Capability maturity / 实际成熟度

| Area | Implemented / 已实现 | Remaining / 尚未成熟 |
| --- | --- | --- |
| Device workbench | Real Android and Linux screens/input, per-device conversations, file exchange, grounded action checks | Full platform/audio/touch/accessibility matrix and long-running multi-device soak |
| Model transport | Responses and Chat Completions, image/tool conversion, configuration probes, cancellation | DeepSeek Chat thinking/tool continuity, discovery, provider-specific policies, native Anthropic/Gemini |
| Channels | QQ, WeCom, Feishu and Telegram adapter code, per-Agent pairing and delivery state | Real-account send/receive/notification/image acceptance; incoming attachments; more platforms |
| Local deployment | macOS ARM64 managed client setup + source CLI, durable installer progress and owned-service resume | Complete fresh-host first run; Windows/Linux host adapters; Android host feasibility |
| State and recovery | Scoped action ledger, runtime/migration DB roles, isolated backup/recovery and failure tests | Signed update activation/rollback, real host sleep/reboot/power-loss and prolonged operation |
| UI and language | Shared controls and eight language catalogs | Large component ownership, complete window/language/error/localization acceptance |

[Compatibility](compatibility.md) defines what was actually checked. **Adapter code, successful HTTP, CI builds and a dropdown entry are different evidence.** No completion percentage is assigned to unmeasured behavior.

## Prioritized remaining work / 按优先级收敛

| ID | Priority | Work and exit criterion / 工作及验收条件 |
| --- | --- | --- |
| H01 | P0 | Client-owned deployment: configure phone/computer/model, prepare services, receive real frames/input/audio, restart without losing state; handle interrupted installation. Per-platform results stay separate. |
| M01 | P1 | DeepSeek Chat thinking: retain genuine provider reasoning for subsequent tool turns, map controls/forced-tool restrictions, preserve tool provenance, prove multi-turn behavior through strict HTTP fixtures and a separately authorized provider check. |
| M02 | P1 | Provider expansion: capability/model discovery, explicit native protocol adapters, bounded errors/cancellation/tool round-trips; never infer support from a provider label. |
| C01 | P1 | Channel certification: real incoming request → selected Agent/device → progress → final text/image; interrupted delivery remains unknown until verified. Add incoming files only with scoped storage and receipt tests. |
| R01 | P1 | Updates/recovery: versioned artifacts, verified manifests, user-state separation, migrations, activation/rollback and host sleep/reboot tests. A direct `git pull` into user data is not an updater. |
| U01 | P1 | UI ownership: split device session, media/input, window lifecycle and registry form state with behavioral tests; verify desktop/narrow/reduced-motion and language layouts. |
| L01 | P1 | Distribution: per-artifact legal review/signing, complete SDK/guest redistribution obligations and clear untested-platform labels. Preview waivers do not close product gates. |
| D01 | P2 | Repository hygiene: small scoped changes, one current release-status owner, accurate compatibility tables and privacy-safe issue reports; retain historical audit evidence in dated reports. |

## This source review / 本次源码修复

Model transport now shares one bounded SSE framing implementation, supports standard multiline events and CR/LF boundaries, refuses non-SSE HTTP success, accepts matching complete endpoint URLs once, and rejects endpoint/protocol mismatches. Configuration files, credentials, chat records and protobuf contracts remain unchanged. This does not close M01/M02 or certify a new public provider.

Runtime verification: assistant tests **71 passed / 30 explicitly ignored**, including loopback HTTP and parser regressions; strict assistant Clippy passed. Ignored database/provider/process tests are not passed tests. The before/after regressions reproduce multiline stream failures, double-appended endpoint 404s and acceptance of an invalid response MIME type. Broader product/runtime acceptance remains open.

## Gates and evidence / 门槛与证据

`config/release-policy.json` owns promotion gates. UI acceptance, complete asset/source redistribution review, long-running host recovery and clean-host installation remain open. Runtime DB role, durable action retention and isolated backup/recovery have scoped passing evidence. `./ai-phone package --public` remains stricter than publishing an explicitly limited source preview.

- [SOLID audit and remaining boundaries](solid-audit.md)
- [Stability, sound and state recovery](release-stability-2026-10-29.md)
- [Task recovery and install checkpoints](task-recovery.md)
- [Native preview scope and notices](preview-release.md)
- [Database roles](database-roles.md), [backup/restore](backup-restore.en.md), [dependency inventory](dependency-licenses.json)
- [Release contract](release-contract.en.md), [中文交付标准](release-contract.zh-CN.md)

Dated audits retain their original scope and historical findings; this page owns the current summary. No live keys, accounts, conversations, guest disks or private screenshots belong in repository evidence or Issues.

## Preview.2 managed-host increment

Native deployment progress is persisted atomically and tied to the bundled payload. A saved completed install is not live readiness: startup resumes owned services and probes before opening. Failed child processes cannot authorize auto-resume via a ready line. Stage completion cannot strand the UI in a false terminal state. Pending licenses are never auto-accepted after restart. Stop interrupts stalled HTTP headers/body without repeating requests, and registered stopped services can start without force-killing live tasks.

Local native regression tests and isolated installer/source-staging tests cover these contracts. Real existing-environment verification retained all seven running service PIDs and returned a genuine desktop frame. This is meaningful H01 implementation evidence, while the fresh-host and independent-platform exit criteria remain open. See [managed-host scope](integrated-hosts.md). No maturity percentage is assigned.
