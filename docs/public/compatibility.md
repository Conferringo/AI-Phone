# Compatibility / 兼容矩阵

2026-10-28. Transport compatibility is not model/task success. Each Agent must pass text, vision and complete tool round-trip probes before saving a changed connection.

## Models / 模型

| Protocol | Implementation | Evidence | Boundary |
| --- | --- | --- | --- |
| Responses | Existing engine transport | Private local model previously passed real text/vision/tool probes | Model IDs and API versions belong to the deployment provider |
| Chat Completions | New input/tool/image translation + bounded SSE parser | Local real-HTTP mock round-trip, split Unicode, completion/truncation tests | No new external provider/account certification in this batch |
| OpenAI-compatible local or cloud gateway | User-supplied base URL + chosen protocol | Must pass connection probes | Compatibility label is not endorsement of every gateway/model |
| Anthropic/Gemini native protocols | Not implemented | None | A user-managed compatible bridge may work after probing; native support is not claimed |

API URL includes the provider's version prefix if needed, not the final `/responses` or `/chat/completions` suffix. Choose the protocol in Agent → connection options. No automatic protocol fallback after an error: it could duplicate requests and hide incompatible behavior. `max_tokens`, images and forced tools must be supported by the Chat Completions endpoint. Provider reasoning controls are not sent indiscriminately.

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

Knowledge, memory editors, autonomous settings, detailed traces, pet text and many backend error strings still need translation. User content and actual device UI are never silently translated. Other languages/RTL/localized date formats are not supported yet. Browser visual acceptance is pending; typed/build tests do not prove English text fits every narrow window.
