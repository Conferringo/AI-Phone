# Models and connected applications

Configure a model and applications inside **Configuration → Agent**. Save several profiles and choose one active Agent for both devices. Memory and channel settings stay with each profile.

## Models

### Direct provider APIs

| Preset | Protocol | Endpoint | Documented model IDs |
|---|---|---|---|
| DeepSeek | Chat Completions | `https://api.deepseek.com` | `deepseek-flash` (V4.1 Flash 1017) |
| OpenAI | Responses | `https://api.openai.com/v1` | `gpt-6.1-terra`, `gpt-6.1-sol`, `gpt-6-astra`, `gpt-6-luna` |
| Anthropic | Native Messages | `https://api.anthropic.com/v1` | `claude-sonnet-5-5`, `claude-opus-5-5`, `claude-haiku-5-5`, `claude-fable-5-1` |
| Gemini | Chat Completions compatibility | `https://generativelanguage.googleapis.com/v1beta/openai` | `gemini-3.8-flash`, `gemini-4-argon` (paid API) |
| xAI · Grok | Responses compatibility | `https://api.x.ai/v1` | `grok-4.7` |
| Meta · Muse | Responses compatibility | `https://api.meta.ai/v1` | `muse-wonder-1.0`, `muse-spark-1.3` |
| Kimi Global / CN | Chat Completions | `https://api.moonshot.ai/v1` / `https://api.moonshot.cn/v1` | `kimi-k3.1-pro` |
| GLM CN / Z.ai | Chat Completions | `https://open.bigmodel.cn/api/paas/v4` / `https://api.z.ai/api/paas/v4` | `glm-4.6v` (CN), `glm-5.3-flash` and `glm-5.3-flashx` (Z.ai) |
| Qwen CN / Global | Chat Completions | `https://dashscope.aliyuncs.com/compatible-mode/v1` / `https://dashscope-intl.aliyuncs.com/compatible-mode/v1` | `qwen3.8-flash`, `qwen3.8-max` and `qwen3.8-max-0902` where offered by the selected region |
| MiniMax | Messages compatibility | `https://api.minimax.io/anthropic/v1` | `MiniMax-M3.1-Flash` |
| Tencent · Hunyuan | Chat Completions | `https://tokenhub.tencentmaas.com/v1` | `hy4` |
| Xiaomi · MiMo | Chat Completions | `https://api.xiaomimimo.com/v1` | `mimo-v2.6-flash`, `mimo-v2.6-pro` |
| Ant · Ling | Messages compatibility | `https://api.ant-ling.com/anthropic/v1` | `Ling-3.2-flash` |
| StepFun CN / Global | Chat Completions | `https://api.stepfun.com/v1` / `https://api.stepfun.ai/v1` | `step-5-preview` |
| Mistral | Responses compatibility | `https://api.mistral.ai/v1` | `mistral-large-4` |
| NVIDIA · NIM | Chat Completions | `https://integrate.api.nvidia.com/v1` | `nvidia/nemotron-3.5-super-vl-120b-a12b` |

### Gateways and local inference

| Preset | Protocol | Endpoint |
|---|---|---|
| OpenRouter | Chat Completions | `https://openrouter.ai/api/v1` |
| Ollama | Chat Completions | `http://127.0.0.1:11434/v1` |
| Custom | Responses, Chat Completions, or Messages | User supplied |

Choose a preset, enter the model ID and API key, then test and save. The device workbench connection check uses a text response, image interpretation, and a complete function call and tool-result round trip. For gateways and self-hosted servers, choose a route that provides all three capabilities. Each regional endpoint uses its own credentials. The advanced proxy applies only to that connection.

The first 20 entries in the November 9, 2026 Intelligence Index contain nine model variants once effort levels are grouped. Seven have documented direct API IDs in the table. [Artificial Analysis](https://artificialanalysis.ai/leaderboards/models)

- Gemini 4 Carbon is in Google's invited partner rollout. Google lists `gemini-4-argon` for paid API access. [Carbon](https://blog.google/innovation-and-ai/models-and-research/gemini-models/introducing-gemini-4-carbon/), [Gemini API pricing](https://ai.google.dev/gemini-api/docs/pricing)
- Muse Wonder Full is available through Muse Code. Meta's Model API offers `muse-wonder-1.0`. [Meta model catalog](https://dev.meta.ai/docs/models)
- DeepSeek V4 Pro, GLM-5.3, and Ling 3.1 Flash take text input. Use `deepseek-flash`, Z.ai's visual Flash series, or `Ling-3.2-flash` for image input. [DeepSeek model capabilities](https://api-docs.deepseek.com/quick_start/pricing/), [Z.ai visual Flash](https://docs.z.ai/guides/vlm/glm-5.3-flash), [Ant Ling model catalog](https://developer.ant-ling.com/en/docs/faq/)
- Thinking Machines exposes Inkling 2 Small through Tinker's token-sampling SDK. That interface is outside the three model protocols above. [Tinker Inkling guide](https://tinker-docs.thinkingmachines.ai/cookbook/inkling/)

After correcting a model ID, API endpoint or key, test and save the configuration in the original Agent. Choose **Resume** on its paused task, or send a follow-up in that task’s conversation. Recovery uses that Agent’s latest saved configuration; finish or stop the current task before switching profiles. Running tasks keep their existing model snapshot until they pause and you resume them. Conversation and execution records are retained. If the replacement configuration is unavailable or unverified, the task remains paused. [Task recovery](task-recovery.md) describes the execution contract.

Native Messages sends `x-api-key` and `anthropic-version`, converts tool results/images and requires a complete stream before returning proposed actions. Signed thinking blocks remain private, verbatim replay data. Chat Completions retains supported providers' reasoning fields and Gemini tool signatures. DeepSeek forced tools disable thinking for that call, preserving the forced tool choice. The shared engine still enforces real observations, execution ownership and post-action evidence.

Local HTTP fixtures cover authentication, image/tool round trips and stream framing. The connection check runs against the configured endpoint and reports account, protocol and capability errors.

## Local WhatsApp pairing

1. Open the Agent's **Connected applications → WhatsApp**, then **Bind**. The server generates a local session; no Meta App ID, business account or public webhook is required.
2. Scan the QR from WhatsApp's **Linked devices**. The adapter uses the unofficial WhatsApp Web protocol.
3. From a separate private account, send the displayed `/pair …` command to the linked WhatsApp account. Send Agent commands from that paired account in private chat.
4. Send private instructions. Text and outgoing PNG screenshots use the existing durable queue. Unknown delivery results are retained for reconciliation and never automatically resent. Incoming attachment/file ingestion is not implemented yet.

An optional proxy is under **Advanced**. Empty uses the host's configured default (direct in a normal deployment). QR payloads and linked sessions are credentials: they remain in authenticated local responses and ignored private storage, never in logs, source bundles or screenshots published as evidence.

**Rescan** removes that session locally and starts a new identity/owner pairing. **Unbind** attempts remote logout and removes the local session. If remote confirmation is unknown, the interface reports it; check WhatsApp's Linked devices and remove the old session there. Network reconnect keeps the same local session; shutdown stops the active connection.

The Rust assistant runs the WhatsApp, QQ, WeCom, Feishu and Telegram adapters. WhatsApp stores its linked-device session in local PostgreSQL. On upgrades from 0.1.5, scan the new WhatsApp QR once. Existing AI Phone conversations and Agent settings stay in place. [Supported channels and operations](compatibility.md)

## Primary references

- [OpenAI model catalog](https://developers.openai.com/api/docs/models), [Claude model catalog](https://platform.claude.com/docs/en/models/overview), [Anthropic streaming](https://platform.claude.com/docs/en/build-with-claude/streaming).
- [DeepSeek thinking/tool continuity](https://api-docs.deepseek.com/guides/thinking_mode/).
- [Kimi international quick start](https://platform.kimi.ai/docs/get-your-api-key) and [China](https://platform.kimi.com/docs/get-api-key).
- [Z.ai completion](https://docs.z.ai/api-reference/llm/chat-completion), [vision/tools](https://docs.z.ai/guides/vlm/glm-4.6v).
- [Gemini OpenAI compatibility](https://ai.google.dev/gemini-api/docs/openai), [Qwen regional endpoints](https://help.aliyun.com/en/model-studio/base-url), [MiniMax Messages](https://platform.minimax.io/docs/api-reference/text-anthropic-api).
- [Grok 4.7](https://docs.x.ai/developers/grok-4-7), [Meta Responses](https://dev.meta.ai/docs/protocols/responses), [Tencent TokenHub](https://cloud.tencent.com/document/product/1823/132252), [Xiaomi MiMo](https://mimo.mi.com/docs/en-US/api/chat/openai-api), [Ant Ling Messages](https://developer.ant-ling.com/en/docs/api-reference/chat/anthropic/).
- [StepFun vision](https://platform.stepfun.com/cookbook/step-5-vision-structured), [Mistral Large](https://docs.mistral.ai/models/mistral-large), [NVIDIA Nemotron](https://build.nvidia.com/nvidia/nemotron-3.5-super-vl-120b-a12b?section=deploy), [OpenRouter](https://openrouter.ai/developers).
- [Native WhatsApp linked-device implementation](https://github.com/jlucaso1/whatsapp-rust).

---

# 模型与连接应用

在「配置中心 → Agent」设置模型、连接应用与记忆。手机与电脑使用同一个当前 Agent。上表列出服务商直连、网关和本地推理的地址、协议及模型 ID。选择预设，填写密钥，通过文字、图片识别和完整的函数调用往返检查后保存。已有账号、设备、配置和聊天保留。不同地区的端点使用各自的密钥。

AA 榜单核对以 2026 年 11 月 9 日为准。Gemini 4 Carbon 处于受邀开放阶段，Argon 可使用付费 Gemini API。Muse Wonder Full 属于 Muse Code，Meta API 提供 `muse-wonder-1.0`。DeepSeek V4 Pro、GLM-5.3 和 Ling 3.1 Flash 为纯文本；设备操作请分别选择 `deepseek-flash`（1017）、GLM 视觉系列或 `Ling-3.2-flash`。Inkling 2 Small 使用 Tinker 的采样接口。 [AA 榜单](https://artificialanalysis.ai/leaderboards/models)

修正模型 ID、API 地址或密钥后，在原 Agent 中测试并保存配置，再点击暂停任务的「继续任务」，或在该任务的对话中补充指令。恢复时使用原 Agent 最新保存的配置；切换配置前先完成或停止当前任务。正在运行的任务保持原模型快照，直到暂停后由你恢复。对话和执行记录保留；替换配置不可用或未通过验证时，任务继续保持暂停。执行契约见[任务恢复](task-recovery.md)。

WhatsApp 采用用户选择的本地扫码关联设备，无需公网回调。入口在 Agent → 连接应用 → WhatsApp。扫码后，从另一个私聊账号发送界面上的配对指令，再进行远程操作；不接收群聊、状态、自发消息或“发给自己”。文字和PNG回复进入原持久任务队列，未知送达不自动重发，入站附件暂未支持。高级项可选代理；用户浏览器资料不会被复用。解绑/重扫会清理当前本地会话；远端注销不确定时会明确提示。

[渠道与操作](compatibility.md)
