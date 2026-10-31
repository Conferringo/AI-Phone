# Container browser lifecycle

Status: **isolated runtime and SDK lifecycle verified; complete installer selection is not wired yet**. The current production connection still selects the host browser. This component is a step toward removing a second host Chromium download from complete distributions, not a complete offline client.

The prototype uses Chromium already present in the pinned, clean Linux desktop image. A separate owned container receives only the selected linked-device profile. It has no published debugging port, Docker socket or other account mounts. Puppeteer launches the bundled Python adapter through its standard executable interface; binary CDP bytes flow through inherited descriptors and Docker stdin/stdout, with bounded chunks and OS backpressure. No per-frame JSON wrapper, database write or model request is added.

## Ownership and recovery

- [`browser_lease.py`](../../../scripts/browser_lease.py) owns the profile's kernel lock, persistent container intent and launch token. It records intent before creation and saves the immutable container ID afterward. An uncertain create is observed before any new create. A container is removed only after image, profile, token and immutable ID agree.
- [`browser_engine.py`](../../../scripts/browser_engine.py) owns Docker command construction and observation. Successful absence differs from daemon failure. It never pulls an image, publishes a port or mounts the engine socket into Chromium. The engine's actual profile UID/GID mapping determines the process user; account permissions are not widened.
- [`browser_pipe.py`](../../../scripts/browser_pipe.py) owns bounded byte transport and guest process cleanup. Normal shutdown allows profile flushing before escalation. [`channel_browser.py`](../../../scripts/channel_browser.py) adapts Puppeteer's launch/stop/erase contract.
- After proving the previous container absent under the profile lock, a new launch may unlink only recognized Chromium IPC symlinks. Their targets and account files remain untouched. Unknown files fail explicitly. This repairs the reproduced exit-21 failure after a forced container crash, where Chromium's old hostname lock survived.
- Logout requires the expected launch owner. A delayed old stop cannot close a newer connection; a delayed old logout cannot erase its account. Markers are path independent for an explicitly restored profile, while transient locks/container names are bound to its current canonical path. Existing unmarked host profiles are rejected by this runtime rather than silently converted or downgraded.

## SDK compatibility repair

`whatsapp-web.js` 1.34.7 calls the removed Puppeteer `isConnected()` method during destroy/logout. The project's security pin is Puppeteer 25.16.0. The project-owned [`WhatsAppClient`](../../../apps/channels/whatsapp-client.ts) preserves SDK initialization and messaging while owning shutdown through `Browser.close()` and the authentication strategy. No third-party source is patched. An unknown close result preserves the account; the channel controller reports the failure and blocks automatic reconnection. Overlapping stop and late initialization share one close promise.

## Evidence and limits

On macOS ARM64, bundled CPython 3.13.18 launched Chromium 156.0.8078.47 in an isolated Linux ARM64 container with `network=none`. A local intercepted page produced real PNG bytes, retained localStorage across normal restarts, and recovered after separately killing the host launcher and the owned container. Real SDK `initialize()` with LocalAuth ran twice against an intercepted fixture and closed through the project adapter. A pre-aborted SDK launch was rejected. No real WhatsApp account, QR scan, message or production profile was used.

The fixture overrides web-page injection/cache acquisition only; it does not qualify WhatsApp authentication or delivery. Windows descriptor inheritance, Linux/Android host integration, production runtime selection, reachable proxy policy, locked container auth deletion, initialization phase ordering and complete bundle assembly remain open. Chromium notices and corresponding-source obligations remain binary publication gates. The [runtime bundle standard](../runtime-bundles.md) and [SOLID audit](../solid-audit.md) remain authoritative for broader readiness.

## Scoped review

The batch ran 328 Python tests (312 passed, 16 explicitly skipped), 172 Bun tests and all TypeScript checks. The four new Python modules and thin SDK adapter have cyclomatic complexity at most 8 and measured CRAP at most 8.3 using executable-line coverage, not branch coverage. Dryer found no candidates at similarity 0.8. All 134 executed Python mutations and four SDK-adapter mutations were caught; eight Python sites remain uncovered, including guest descriptor mapping and dispatch/attach edges. The older channel controller still has CC/CRAP 52 and remains refactoring debt. These scoped results do not qualify the whole repository, Windows browser transport or complete product.

## 中文摘要

已验证随包 Python、标准 Puppeteer/WhatsApp SDK 启动、真实画面、持久档案、强制崩溃恢复与有归属的退出；修复 SDK 与 Puppeteer25 的关闭兼容问题，关闭未知时保留账号并阻止重连。容器浏览器尚未接入完整安装器的生产选择，当前渠道仍选宿主浏览器。没有使用真实账号，不能据此声称扫码/消息收发或四平台完整客户端通过。
