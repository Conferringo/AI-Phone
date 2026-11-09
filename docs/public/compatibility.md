# Compatibility / 兼容矩阵

An Agent's device workbench connection is verified with text, image interpretation and a complete function call and tool-result round trip. Installer availability is listed in the [release status](release-status.md).

## Models / 模型

| Protocol | Implementation | Evidence | Boundary |
| --- | --- | --- | --- |
| Responses | Engine transport and shared SSE decoder | Local HTTP and stream framing fixtures | Native OpenAI API and compatible xAI, Meta and Mistral endpoints |
| Chat Completions | Message, tool and image translation with shared SSE decoder | Local HTTP round trips, split Unicode, multiline data and terminal frames | Direct APIs, hosted model endpoints, and gateways such as OpenRouter |
| Anthropic Messages | API-key authentication, images, tool use/results and private signed-block replay | Local HTTP authentication, image and tool round trips, and terminal frames | Native Anthropic API and compatible MiniMax and Ant Ling endpoints |
| Gemini | Official OpenAI-compatible Chat Completions endpoint with opaque tool-signature replay | Translation fixtures | Model access follows the Google account and selected API |

API URL may include the provider's version prefix and may also include the complete `/responses`, `/chat/completions` or `/messages` endpoint. A matching suffix is used once; a suffix that conflicts with the selected protocol is rejected. Saved configuration and credentials are not rewritten. Choose the protocol in Agent → connection options. `max_tokens`, images and forced tools must be supported by the Chat Completions endpoint.

The decoder accepts LF, CRLF or CR line boundaries, comments, initial BOM, split UTF-8 and multiline `data:` fields. Only a blank line completes an SSE event. The protocol must then supply a valid terminal result; EOF/partial tools remain failures. HTTP success without `text/event-stream` is rejected. Responses and Chat Completions share framing and size limits, while keeping separate completion/tool rules. See the [SSE standard](https://html.spec.whatwg.org/multipage/server-sent-events.html#event-stream-interpretation).

`config/model-providers.json` supplies the Agent picker and server-side provider list. The [model setup guide](model-and-channel-setup.md) lists editable endpoint and model IDs, the Artificial Analysis comparison and the available API variants. Text-only variants such as DeepSeek V4 Pro, GLM-5.3 and Ling 3.1 Flash cannot pass the image probe. OpenRouter and self-hosted routes use the same verification as direct provider APIs.

DeepSeek Chat preserves returned `reasoning_content` for completed native turns and maps thinking/effort controls. Forced/required tools disable thinking for that request while preserving the requested tool constraint, as required by its API. Kimi/GLM preserve their returned reasoning; GLM receives only the required tool when its API supports auto choice alone. Gemini's opaque tool signatures survive a tool round-trip. The engine uses private reasoning for model replay and keeps legacy or interrupted calls in the execution record. [DeepSeek contract](https://api-docs.deepseek.com/guides/thinking_mode/), [tool restrictions](https://api-docs.deepseek.com/api/create-chat-completion/).

The evidence column describes local coverage. The Agent connection check verifies each external account and model endpoint.

Images from tool results are preserved as evidence in a following user content message for Chat Completions; tool-call IDs remain intact. Truncated, filtered, malformed or unterminated responses cannot produce executable tools. The execution engine's current-observation and completion checks still apply. Reference: [official Chat Completions API](https://developers.openai.com/api/reference/resources/chat).

## Channels / 渠道

| Channel | Adapter | Pairing and content |
| --- | --- | --- |
| QQ | Native Rust gateway WebSocket | Private pairing, text and outgoing screenshots |
| WeCom 企业微信 | Native Rust outbound WebSocket | Per-Agent pairing |
| Feishu 飞书 | Native Rust protobuf WebSocket | Per-Agent pairing |
| Telegram | Native Rust Bot API with long polling | Bot Token, private `/pair`, text and outgoing PNG |
| WhatsApp | Unofficial native Rust linked-device adapter | QR login, private `/pair`, text and outgoing PNG, local session storage |

Discord and Slack are planned.

Telegram checks the bot identity, refuses an existing Webhook, persists queued events before advancing the polling offset and never auto-replays unknown sends. Long-polling authentication/conflict errors are surfaced. It does not delete another application's Webhook. Incoming images/files are not implemented; outgoing text is bounded to the platform limit. Official reference: [Bot API getUpdates](https://core.telegram.org/bots/api#getupdates).

Bindings are per Agent, not per model credential. Two Agents may use the same model connection while keeping separate memories and bot ownership. Each sender must be authorized through private pairing.

## Languages / 语言

Ten UI catalogs cover simplified/traditional Chinese, English, Japanese, Korean, German, French, Spanish, Russian and Arabic. The main language picker sits immediately before Skills; Arabic sets RTL. Labels switch without reloading or clearing chats/drafts. Native initial setup also offers these languages. User content and guest OS language are not silently changed.

Backend detailed errors and some tool records still contain source-language strings. [Language coverage](languages.md).

## Host platforms / 独立部署

See [downloads and platform status](release-status.md).
## Development runtime: exploration and notifications

The source runtime supports [self-directed exploration](autonomous-exploration.md) with one phone/desktop policy and persisted resource windows, plus [device notification wake and delegated work](notification-workflows.md). Independent exploration, owner tasks and notifications have separate scheduling lanes. See [platform status](release-status.md).
