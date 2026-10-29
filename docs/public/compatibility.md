# Compatibility / 兼容矩阵

2026-10-29. **Experimental, not broad provider certification. / 实验性兼容，不代表所有服务商均已验收。** Each Agent must pass text, vision and complete tool round-trip probes before saving a changed connection. These probes do not certify every thinking mode or every long-running task. This source change is ahead of the published preview.1 binaries.

## Models / 模型

| Protocol | Implementation | Evidence | Boundary |
| --- | --- | --- | --- |
| Responses | Engine transport + shared bounded SSE decoder | Private local model previously passed real text/vision/tool probes; current local HTTP/framing regressions | Model IDs and API versions belong to the deployment provider |
| Chat Completions | Input/tool/image translation + shared bounded SSE decoder | Local real-HTTP round-trip, split Unicode, multiline data, completion/truncation tests | No new external provider/account certification in this batch; thinking-mode continuity is incomplete |
| OpenAI-compatible local or cloud gateway | User-supplied base URL + chosen protocol | Must pass connection probes | Compatibility label is not endorsement of every gateway/model |
| Anthropic/Gemini native protocols | Not implemented | None | A user-managed compatible bridge may work after probing; native support is not claimed |

API URL may include the provider's version prefix and may also include the complete `/responses` or `/chat/completions` endpoint. A matching suffix is used once; a suffix that conflicts with the selected protocol is rejected. Saved configuration and credentials are not rewritten. Choose the protocol in Agent → connection options. There is no automatic protocol fallback after an error: it could duplicate requests and hide incompatible behavior. `max_tokens`, images and forced tools must be supported by the Chat Completions endpoint.

The decoder accepts LF, CRLF or CR line boundaries, comments, initial BOM, split UTF-8 and multiline `data:` fields. Only a blank line completes an SSE event. The protocol must then supply a valid terminal result; EOF/partial tools remain failures. HTTP success without `text/event-stream` is rejected. Responses and Chat Completions share framing and size limits, while keeping separate completion/tool rules. See the [SSE standard](https://html.spec.whatwg.org/multipage/server-sent-events.html#event-stream-interpretation).

**Known DeepSeek Chat limitation / 已知高优先级缺口:** the current translator drops `reasoning_content` and does not map thinking controls fully. DeepSeek requires reasoning to be returned on subsequent tool turns and restricts forced tool choices in thinking mode; an ordinary text test cannot certify this. Existing configurations are preserved. Use a separately verified Responses connection for now; do not claim DeepSeek Chat thinking-mode Agent loops are complete. [Official thinking-mode contract](https://api-docs.deepseek.com/guides/thinking_mode/), [tool-choice restrictions](https://api-docs.deepseek.com/api/create-chat-completion/).

Pending model work: provider-specific thinking/tool policies with full round-trip fixtures; model discovery and capability metadata; native Anthropic/Gemini adapters; structured/localized error codes; real external-provider acceptance. Adding a provider name to a dropdown is not acceptance.

Images from tool results are preserved as evidence in a following user content message for Chat Completions; tool-call IDs remain intact. Truncated, filtered, malformed or unterminated responses cannot produce executable tools. The execution engine's current-observation and completion checks still apply. Reference: [official Chat Completions API](https://developers.openai.com/api/reference/resources/chat).

## Channels / 渠道

| Channel | Adapter | Pairing / content | Verification |
| --- | --- | --- | --- |
| QQ | Official bot SDK / WebSocket | Private pairing; text + outgoing screenshot | SDK/interface/isolation tested; real bot credentials pending |
| WeCom 企业微信 | Official AI bot SDK / outbound connection | Same Agent-level pairing | Real-account certification pending |
| Feishu 飞书 | Official SDK / long connection | Same Agent-level pairing | Real-account certification pending |
| Telegram | Official Bot API / outbound long polling | Bot Token only; private `/pair`; text + outgoing PNG | New isolated adapter tests; real token round-trip pending |
| Discord/Slack/others | Planned | Same capability/owner-delivery contract intended | No buttons or compatibility claims yet |

Telegram checks the bot identity, refuses an existing Webhook, persists queued events before advancing the polling offset and never auto-replays unknown sends. Long-polling authentication/conflict errors are surfaced. It does not delete another application's Webhook. Incoming images/files are not implemented; outgoing text is bounded to the platform limit. Official reference: [Bot API getUpdates](https://core.telegram.org/bots/api#getupdates).

Bindings are per Agent, not per model credential. Two Agents may use the same model connection while keeping separate memories and bot ownership. Removing a visible allowed-user-ID field does not authorize every sender; private pairing remains required.

## Languages / 语言

Eight catalogs cover simplified/traditional Chinese, English, Japanese, Korean, German, French and Spanish. Session startup, configuration, model/device installation, shared windows, connection forms, execution controls and primary device toolbar use the same translation context. One language choice lives inside Configuration and on the optional password/setup screens. It changes labels without reloading or clearing chat/drafts; the preference is a cookie, not a model setting.

Knowledge, memory editors, autonomous settings, detailed traces and many backend error strings still need translation. User content and actual device UI are never silently translated. Other languages/RTL/localized date formats are not supported yet. Browser visual acceptance is pending; typed/build tests do not prove English text fits every narrow window.

## Host platforms / 独立部署

| Platform | Published client | Independent local host |
| --- | --- | --- |
| macOS Apple Silicon | Connection client | Source CLI exists; integrated first-run installer remains in development, fresh-host acceptance pending |
| Windows x64 | Connection client | WHPX/WSL2 host adapter and service lifecycle not implemented |
| Linux x64 / ARM64 | Connection client | KVM/user-service host adapter and macOS vision replacement not implemented |
| Android ARM64 | Connection client | Privileged AVF/KVM and software-emulation research; ordinary APK dual-guest hosting is not promised |

Five built installer artifacts do not establish four standalone hosts. [Release gates and priorities](release-status.md) are the current completion record. Physical-install acceptance was waived for the experimental preview and remains untested, not passed.
