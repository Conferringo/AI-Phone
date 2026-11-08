# Compatibility / 兼容矩阵

**Experimental provider compatibility / 实验性服务商兼容。** Each Agent must pass text, vision and complete tool round-trip probes before saving a changed connection. See the [release status](release-status.md) for installer availability.

## Models / 模型

| Protocol | Implementation | Evidence | Boundary |
| --- | --- | --- | --- |
| Responses | Engine transport + shared bounded SSE decoder | Private local model previously passed real text/vision/tool probes; current local HTTP/framing regressions | Model IDs and API versions belong to the deployment provider |
| Chat Completions | Input/tool/image translation + shared bounded SSE decoder | Local real-HTTP round-trip, split Unicode, multiline data, completion/truncation tests | Native reasoning replay for DeepSeek/Kimi/GLM and Gemini tool signatures; new external-provider acceptance still pending |
| OpenAI-compatible local or cloud gateway | User-supplied base URL + chosen protocol | Must pass connection probes | Compatibility label is not endorsement of every gateway/model |
| Anthropic Messages / MiniMax | Native API-key authentication, images, tool use/results, private signed-block replay, bounded SSE | Real local HTTP/auth/image/tool round-trip and truncation fixtures | No new paid-provider account acceptance |
| Gemini | Official OpenAI-compatible endpoint, opaque tool-signature replay | Translation fixtures | Native generateContent protocol is not implemented |

API URL may include the provider's version prefix and may also include the complete `/responses`, `/chat/completions` or `/messages` endpoint. A matching suffix is used once; a suffix that conflicts with the selected protocol is rejected. Saved configuration and credentials are not rewritten. Choose the protocol in Agent → connection options. There is no automatic protocol fallback after an error: it could duplicate requests and hide incompatible behavior. `max_tokens`, images and forced tools must be supported by the Chat Completions endpoint.

The decoder accepts LF, CRLF or CR line boundaries, comments, initial BOM, split UTF-8 and multiline `data:` fields. Only a blank line completes an SSE event. The protocol must then supply a valid terminal result; EOF/partial tools remain failures. HTTP success without `text/event-stream` is rejected. Responses and Chat Completions share framing and size limits, while keeping separate completion/tool rules. See the [SSE standard](https://html.spec.whatwg.org/multipage/server-sent-events.html#event-stream-interpretation).

Provider presets are centralized in `config/model-providers.json`: DeepSeek, OpenAI, Anthropic, Kimi Global/CN, GLM CN/Z.ai, Gemini, Qwen CN/Global, MiniMax, Ollama and custom. All endpoints and model IDs remain editable. Regional API keys are not interchangeable. [Configuration and sources](model-and-channel-setup.md).

DeepSeek Chat preserves returned `reasoning_content` for completed native turns and maps thinking/effort controls. Forced/required tools disable thinking for that request while preserving the requested tool constraint, as required by its API. Kimi/GLM preserve their returned reasoning; GLM receives only the required tool when its API supports auto choice alone. Gemini's opaque tool signatures survive a tool round-trip. Private reasoning is replay data, not UI output. Legacy or interrupted calls remain quoted evidence and are not represented as newly executed actions. [DeepSeek contract](https://api-docs.deepseek.com/guides/thinking_mode/), [tool restrictions](https://api-docs.deepseek.com/api/create-chat-completion/).

Pending model work: discovery/capability metadata, structured/localized provider errors, native Gemini generateContent, and actual acceptance with each external provider.

Images from tool results are preserved as evidence in a following user content message for Chat Completions; tool-call IDs remain intact. Truncated, filtered, malformed or unterminated responses cannot produce executable tools. The execution engine's current-observation and completion checks still apply. Reference: [official Chat Completions API](https://developers.openai.com/api/reference/resources/chat).

## Channels / 渠道

| Channel | Adapter | Pairing / content | Verification |
| --- | --- | --- | --- |
| QQ | Native Rust gateway WebSocket | Private pairing; text + outgoing screenshot | Compiled; real bot credentials and message round-trip pending |
| WeCom 企业微信 | Native Rust outbound WebSocket | Same Agent-level pairing | Compiled; real-account certification pending |
| Feishu 飞书 | Native Rust protobuf WebSocket | Same Agent-level pairing | Compiled; real-account certification pending |
| Telegram | Native Rust Bot API / long polling | Bot Token only; private `/pair`; text + outgoing PNG | Compiled; real token round-trip pending |
| WhatsApp | Unofficial native Rust linked-device adapter | QR login + private `/pair`; text + outgoing PNG; local session store | Compiled; QR, account pairing, reconnect and real delivery on this adapter remain pending |
| Discord/Slack/others | Planned | Same capability/owner-delivery contract intended | No buttons or compatibility claims yet |

Telegram checks the bot identity, refuses an existing Webhook, persists queued events before advancing the polling offset and never auto-replays unknown sends. Long-polling authentication/conflict errors are surfaced. It does not delete another application's Webhook. Incoming images/files are not implemented; outgoing text is bounded to the platform limit. Official reference: [Bot API getUpdates](https://core.telegram.org/bots/api#getupdates).

Bindings are per Agent, not per model credential. Two Agents may use the same model connection while keeping separate memories and bot ownership. Each sender must be authorized through private pairing.

## Languages / 语言

Ten UI catalogs cover simplified/traditional Chinese, English, Japanese, Korean, German, French, Spanish, Russian and Arabic. The main language picker sits immediately before Skills; Arabic sets RTL. Labels switch without reloading or clearing chats/drafts. Native initial setup also offers these languages. User content and guest OS language are not silently changed.

Backend detailed errors and some tool records still contain source-language strings. Catalog checks are not full linguistic review; a complete ten-language/narrow-screen/media matrix remains open. [Language coverage](languages.md).

## Host platforms / 独立部署

See [downloads and platform status](release-status.md).
## Development runtime: exploration and notifications

The source runtime supports [self-directed exploration](autonomous-exploration.md) with one phone/desktop policy and persisted resource windows, plus [device notification wake and delegated work](notification-workflows.md). Independent exploration, owner tasks and notifications have separate scheduling lanes. See [platform status](release-status.md).
