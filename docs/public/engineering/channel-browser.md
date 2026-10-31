# Linked-device browser runtime

The local WhatsApp connection uses the Linux Chromium already included in the desktop image for new bundled installations. The client verifies that image after import, before starting services. Initialization does not download a second host browser.

## Selection and account storage

[`BrowserSession`](../../../apps/channels/whatsapp-runtime.ts) supplies the same selection policy to installation checks, binding validation and connection startup. A fresh bundled account uses the immutable desktop image, bundled Python and the private stdio launcher. Existing unmarked host profiles retain their original browser; the explicit `AI_PHONE_WHATSAPP_CHROMIUM` contributor override is supported for host profiles. A container-owned profile requires its matching runtime and never silently falls back to a host browser.

Account files live under `user/data/channels/whatsapp`, separate from code releases. SDK authentication callbacks mark revocation; the runtime deletes an account only after closure is confirmed. Stop preserves the account. Explicit logout waits for closure and the owner-bound erase command. Unknown outcomes retain the files and surface a failure.

[`WhatsAppConnection`](../../../apps/channels/whatsapp.ts) owns the connection generation, UI state and reconnect scheduling. Each attempt owns its client, cancellation signal and close promise. Every asynchronous publication and retry checks its generation. Stale QR renders, queued retries and initialization results cannot reactivate a stopped connection. An incoming message is dispatched only after the connection's identity gate succeeds.

## Transport and ownership

Puppeteer launches the bundled Python adapter through its standard executable interface. Binary CDP bytes flow through inherited descriptors and Docker stdin/stdout, using bounded chunks and OS backpressure. No debugging port is published. The browser container receives only its selected profile, no Docker socket or other accounts. Its process user follows the engine's actual profile UID/GID mapping.

- [`browser_lease.py`](../../../scripts/browser_lease.py) owns the kernel lock, persisted container intent and unique launch token. An uncertain create is observed before any new create. Removal requires matching image, profile, token and immutable container ID.
- [`browser_engine.py`](../../../scripts/browser_engine.py) owns Docker commands and observation. Successful absence differs from daemon failure. It never pulls an image. Graceful shutdown allows profile flushing before escalation.
- [`browser_pipe.py`](../../../scripts/browser_pipe.py) owns the bounded byte relay and guest process cleanup. [`channel_browser.py`](../../../scripts/channel_browser.py) adapts launch, stop and erase.

After confirming the previous container absent under the profile lock, launch may unlink recognized stale Chromium IPC symlinks. Their targets and account files remain untouched. Unknown files fail explicitly. Delayed old logout cannot erase a newer connection. Persistent markers survive a code-root change; transient ownership keys follow the canonical data path.

Container connections use direct networking or a container-reachable proxy. Host loopback proxy addresses are rejected with an actionable configuration error. Proxy credentials are passed through browser authentication, separate from process arguments. No proxy failure silently switches to direct networking.

## SDK compatibility

The security-pinned Puppeteer 25 integration uses the project-owned [`WhatsAppClient`](../../../apps/channels/whatsapp-client.ts) for browser closure and authentication callbacks. SDK initialization and message handling remain upstream implementations. Third-party source is unchanged. The SDK's optional archive path is checked with directory traversal, Unicode and hidden files, and pattern exclusions under the locked dependency versions.

## Verification scope

On macOS ARM64, bundled CPython 3.13.18 launched Chromium 156.0.8078.47 in an isolated Linux ARM64 container with `network=none`. Real page rendering, screenshots, profile persistence, host-launcher and container crash recovery, standard SDK initialization, pre-aborted launch, production-controller stop and explicit logout were exercised. Changing the code root while sharing the persistent data directory retained the fixture account; a delayed old logout was rejected.

The compiled installation entry was executed with WAN denied and the original project unreadable except for verified runtime artifacts. Both preparation and verification used the image browser, without downloading host Chromium or creating account directories. These checks use intercepted local pages: real WhatsApp account authentication and delivery were not exercised. Windows descriptor transport, Android hosting and complete-client assembly retain their platform acceptance requirements. Chromium notices and corresponding-source obligations remain binary publication gates. See [runtime bundles](../runtime-bundles.md), [compatibility](../compatibility.md) and [SOLID audit](../solid-audit.md).

## Scoped code review

The production connection and runtime methods have CC/CRAP at most 7 under executable-line coverage; the host-browser selector measures 9/9.1. The existing proxy URL function remains CC/CRAP 10. The changed installation and source-input functions have CC at most 7 and CRAP below 8. Dryer found no candidates at similarity 0.8 in the connection/runtime boundary. Of 104 TypeScript mutation sites, 99 were caught; remaining review items cover Windows console visibility, SDK QR retry configuration, and equivalent or declared-SDK-contract expressions. This is not whole-repository or branch-coverage qualification. Larger installer validation/CLI functions retain separate audit debt.

## 中文摘要

完整包的新账号使用随附 Linux 镜像里的 Chromium，安装器在镜像导入后验证，不额外下载宿主浏览器。旧宿主档案保留原运行方式。停止保留档案，登出在确认关闭和归属后删除；未知结果保留数据并反馈失败。真实禁网测试已覆盖标准 SDK、生产控制器、代码目录切换后的档案保持和旧登出拒绝。尚未使用真实 WhatsApp 账号，也不代表全部平台和完整客户端验收完成。
