# Models and connected applications

**Development scope, 2026-10-30.** Configure a model and applications inside **Configuration → Agent**. Device bindings and memories belong to the Agent; model credentials can be shared without sharing channel ownership. Existing saved configurations are retained.

## Models

| Preset | Protocol | Region / endpoint |
|---|---|---|
| DeepSeek | Chat Completions | `https://api.deepseek.com` |
| OpenAI | Responses | `https://api.openai.com/v1` |
| Anthropic | Messages | `https://api.anthropic.com/v1` |
| Kimi Global / CN | Chat Completions | `https://api.moonshot.ai/v1` / `https://api.moonshot.cn/v1` |
| GLM CN / Z.ai | Chat Completions | `https://open.bigmodel.cn/api/paas/v4` / `https://api.z.ai/api/paas/v4` |
| Gemini | Official OpenAI compatibility | `https://generativelanguage.googleapis.com/v1beta/openai` |
| Qwen CN / Global | Chat Completions | `https://dashscope.aliyuncs.com/compatible-mode/v1` / `https://dashscope-intl.aliyuncs.com/compatible-mode/v1` |
| MiniMax | Messages | `https://api.minimax.io/anthropic/v1` |
| Ollama | Chat Completions | `http://127.0.0.1:11434/v1` |
| Custom | Any of the three implemented protocols | User supplied |

Select the preset, enter a model ID and API key, then run the connection check before saving. Preset examples may change availability; choose a model supporting **images and tools** for device control. A text-only service cannot pass the required vision probe. Cloud credentials must match the endpoint's region; keys never move from the previous provider when changing presets. The advanced proxy applies only to that model connection. No protocol fallback silently retries failed requests.

Native Messages sends `x-api-key` and `anthropic-version`, converts tool results/images and requires a complete stream before returning proposed actions. Signed thinking blocks remain private, verbatim replay data. Chat Completions retains supported providers' reasoning fields and Gemini tool signatures. DeepSeek forced tools disable thinking for that call, preserving the forced tool choice. The shared engine still enforces real observations, execution ownership and post-action evidence.

Local real-HTTP fixtures verify authentication, image/tool round-trips and framing. They do **not** establish successful paid-provider account use. Existing local models recently returned HTTP 402; that account/service refusal is not solved by changing the preset list.

## Local WhatsApp pairing

1. Open the Agent's **Connected applications → WhatsApp**, then **Bind**. The server generates a local session; no Meta App ID, business account or public webhook is required.
2. Scan the QR from WhatsApp's **Linked devices**. This is an **unofficial Web protocol adapter**, not Meta Cloud API. Its upstream may change and account/platform restrictions still apply.
3. After connected, send the displayed `/pair …` command from the private account that will own remote commands to the linked WhatsApp account. Only that paired sender can create Agent jobs. Groups, status broadcasts, self-sent messages and notes-to-self are ignored. This requires a separate sender account; linking alone does not authorize all contacts.
4. Send private instructions. Text and outgoing PNG screenshots use the existing durable queue. Unknown delivery results are retained for reconciliation and never automatically resent. Incoming attachment/file ingestion is not implemented yet.

An optional proxy is under **Advanced**. Empty uses the host's configured default (direct in a normal deployment). The browser is project-owned; personal Chrome profiles are never reused. QR payloads and linked sessions are credentials: they remain in authenticated local responses and ignored private storage, never in logs, source bundles or screenshots published as evidence.

**Rescan** removes that session locally and starts a new identity/owner pairing. **Unbind** attempts remote logout and removes the local profile. If remote confirmation is unknown, the interface reports it; check WhatsApp's Linked devices and remove the old session there. Network reconnect keeps the same local profile; shutdown waits for browser cleanup.

Developer/source bootstrap prepares a project Chromium component automatically. Existing source checkouts can run `./scripts/bun apps/channels/install-browser.ts`; `--verify` is offline. A reviewed absolute executable can be supplied through `AI_PHONE_WHATSAPP_CHROMIUM`. Linux ARM source bootstrap currently needs an architecture-matched external/bundled runtime; Android root/KVM integration is not accepted yet. **Complete product distributions must bundle this component and its notices**, just like guest images/emulators. Downloading a developer Chromium is not a completed offline product installer.

Real WhatsApp Web generated a valid QR twice in disposable sessions, including the pinned Puppeteer 25.16.0 override replacing an advisory-affected older dependency chain. No account was scanned and no real message was sent in this verification. Account pairing, restart/reconnect and actual text/image delivery still need account-level acceptance. QQ, WeCom, Feishu and Telegram retain their existing adapters; [the matrix](compatibility.md) distinguishes code evidence from real-account tests.

## Primary references

- [Anthropic Messages](https://platform.claude.com/docs/en/api/typescript/messages/create) and [streaming](https://platform.claude.com/docs/en/build-with-claude/streaming).
- [DeepSeek thinking/tool continuity](https://api-docs.deepseek.com/guides/thinking_mode/).
- [Kimi international quick start](https://platform.kimi.ai/docs/get-your-api-key) and [China](https://platform.kimi.com/docs/get-api-key).
- [Z.ai completion](https://docs.z.ai/api-reference/llm/chat-completion), [vision/tools](https://docs.z.ai/guides/vlm/glm-4.6v).
- [Gemini OpenAI compatibility](https://ai.google.dev/gemini-api/docs/openai), [Qwen regional endpoints](https://help.aliyun.com/en/model-studio/base-url), [MiniMax Messages](https://platform.minimax.io/docs/api-reference/text-anthropic-api).
- [whatsapp-web.js documentation](https://docs.wwebjs.dev/) and [upstream](https://github.com/wwebjs/whatsapp-web.js).

---

# 模型与连接应用

配置中心的每个 Agent 内统一管理模型、设备、连接应用与记忆。已有账号、设备、配置和聊天保留。当前有13项可编辑预设，覆盖 DeepSeek、OpenAI、Anthropic、Kimi 国内/国际、GLM 国内/Z.ai、Gemini、Qwen 国内/国际、MiniMax、Ollama 和自定义；支持 Responses、Chat Completions 与原生 Messages。更换预设不会沿用上一个服务商的密钥。必须选支持图片和工具的模型，并通过实际连接检查。

WhatsApp 采用用户选择的本地扫码关联设备，无需公网回调。入口在 Agent → 连接应用 → WhatsApp。扫码后，从另一个私聊账号发送界面上的配对指令，再进行远程操作；不接收群聊、状态、自发消息或“发给自己”。文字和PNG回复进入原持久任务队列，未知送达不自动重发，入站附件暂未支持。高级项可选代理；用户浏览器资料不会被复用。解绑/重扫会清理当前本地会话；远端注销不确定时会明确提示。

真实浏览器已两次生成有效二维码，未绑定任何真实账号，也未发送测试消息。新服务商缺少实号密钥的验收保持待验证，不将下拉选项视为通过。完整四平台发行包仍需预装浏览器、客体镜像、模拟器和服务组件，不能用开发安装脚本替代最终交付。完整技术与边界、官方资料见上方英文部分。
