# Linked-device browser runtime

The local WhatsApp connection uses the Linux Chromium already included in the desktop image for new bundled installations. The client verifies that image after import, before starting services. Initialization does not download a second host browser.

## Selection and account storage

[`BrowserSession`](../../../apps/channels/whatsapp-runtime.ts) supplies the same selection policy to installation checks, binding validation and connection startup. A fresh bundled account uses the immutable desktop image and native Rust host component. `AI_PHONE_NATIVE_HOST_PROGRAM` comes from the installation's native runtime environment; account storage does not select an executable. Existing unmarked host profiles retain their original browser; the explicit `AI_PHONE_WHATSAPP_CHROMIUM` contributor override is supported for host profiles. A container-owned profile requires its matching runtime and never silently falls back to a host browser.

Account files live under `user/data/channels/whatsapp`, separate from code releases. SDK authentication callbacks mark revocation; the runtime deletes an account only after closure is confirmed. Stop preserves the account. Explicit logout waits for closure and the owner-bound erase command. Unknown outcomes retain the files and surface a failure.

[`WhatsAppConnection`](../../../apps/channels/whatsapp.ts) owns the connection generation, UI state and reconnect scheduling. Each attempt owns its client, cancellation signal and close promise. Every asynchronous publication and retry checks its generation. Stale QR renders, queued retries and initialization results cannot reactivate a stopped connection. An incoming message is dispatched only after the connection's identity gate succeeds.

## Transport and ownership

Puppeteer's executable interface starts the Rust component with `--channel-browser`. A fixed shell `exec` maps its CDP descriptors onto stdin/stdout; it performs no account or lifecycle decisions. Tokio handles binary streaming and OS backpressure for both Unix pipes and the socket pairs used by Bun. The guest similarly wires Chromium's private pipe without Python. No debugging port is published. The browser container receives only its selected profile, no Docker socket or other accounts. Its process user follows the selected engine's actual profile UID/GID mapping, queried with the guest's native `stat`.

- [`channel_browser`](../../../crates/host-runtime/src/channel_browser/mod.rs) owns the kernel lock, persisted container intent, unique launch token and launch/stop/erase commands. An uncertain create is observed before any new create. Removal requires matching image, profile, token and immutable container ID. Existing format-1 markers and recovery records remain readable.
- [`engine.rs`](../../../crates/host-runtime/src/channel_browser/engine.rs) owns Docker commands and observation. Successful absence differs from daemon failure. It never pulls an image.
- [`pipe.rs`](../../../crates/host-runtime/src/channel_browser/pipe.rs) owns cancellation and streaming. Closing input drains pending output and allows Chromium to flush its account profile before cleanup. The profile lease remains held through container shutdown.

After confirming the previous container absent under the profile lock, launch may unlink recognized stale Chromium IPC symlinks. Their targets and account files remain untouched. Unknown files fail explicitly. Delayed old logout cannot erase a newer connection. Persistent markers survive a code-root change; transient ownership keys follow the canonical data path.

Container connections use direct networking or a container-reachable proxy. Host loopback proxy addresses are rejected with an actionable configuration error. Proxy credentials are passed through browser authentication, separate from process arguments. No proxy failure silently switches to direct networking.

## SDK compatibility

The security-pinned Puppeteer 25 integration uses the project-owned [`WhatsAppClient`](../../../apps/channels/whatsapp-client.ts) for browser closure and authentication callbacks. SDK initialization and message handling remain upstream implementations. Third-party source is unchanged. The SDK's optional archive path is checked with directory traversal, Unicode and hidden files, and pattern exclusions under the locked dependency versions.

## Availability

Browser transport, account authentication and message delivery are separate capabilities. Supported platforms and account-level limitations are described in [compatibility](../compatibility.md) and [release status](../release-status.md). Chromium attribution and corresponding-source requirements apply to binary distribution.

## 中文摘要

完整包的新账号使用随附 Linux 镜像里的 Chromium，由 Rust 原生组件管理生命周期与私有字节通道，此路径不再依赖宿主或客体 Python。安装器在镜像导入后验证，不额外下载宿主浏览器。旧宿主档案保留原运行方式。停止保留档案，登出在确认关闭和归属后删除；未知结果保留数据并反馈失败。渠道服务仍有单独的 JavaScript 迁移欠账，不代表整个项目已纯 Rust。账号兼容与当前平台范围见[兼容矩阵](../compatibility.md)和[发行状态](../release-status.md)。
