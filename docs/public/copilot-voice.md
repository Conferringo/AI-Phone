# Copilot voice

Copilot adds continuous microphone input and interruptible spoken replies to the existing conversation. Enable it beside Liberty in the conversation composer. Both modes can remain active; spoken instructions have priority. Switching devices preserves a phrase's original destination; the next phrase follows the focused device. Already accepted tasks continue when capture stops or the view changes.

**Status.** Browser capture, Agent voice settings, Rust transcription, protobuf transport, durable corrections and interruptible playback are implemented. Real microphone and provider acceptance remains open.

## Configure an Agent

Open **Configuration → Agent → Voice**. Enter the compatible API URL, API key, input protocol, recognition model, speech model and voice. The key is editable and can be shown in this form; it is stored in the private database and excluded from ordinary configuration projections. A local compatible service may leave the key empty. Voice configuration has its own revision and does not require a successful chat-model probe. The text/tool model remains independently configured.

| Input protocol | Provider requests | Correction capability |
|---|---|---|
| Live transcription | Persistent `/realtime?intent=transcription` WebSocket with a transcription session | Incremental literal text; only completed turns enter chat. Spoken corrections remain user instructions in the existing continuous conversation. |
| Audio chat | `/chat/completions` with `input_audio` and text output | One request transcribes the phrase and interprets explicit spoken edits. The provider must support this audio-chat contract. |
| Transcription | Multipart `/audio/transcriptions` | Literal transcription with draft updates at short pauses. |

All input modes use `/audio/speech` with WAV output for replies. Select a model supported by the chosen protocol. Live transcription requires a provider model with the transcription-session schema. Other providers' streaming protocols need their own adapters.

## Continuous conversation

- AudioWorklet captures mono audio, locally resampled to 16 kHz PCM16. REST modes do not make a request per frame. Live transcription streams a confirmed utterance over one persistent connection, including its endpoint silence, instead of uploading a complete WAV after endpointing. Rust converts to the provider's 24 kHz PCM format while retaining interpolation state across frames.
- **1,500 ms of microphone silence** seals a phrase. Only final recognized text is committed. Recognition and network time follow that interval.
- Audio-chat interpretation runs once per completed phrase, with a hard 28-second chunk boundary. Transcription mode can send phrase chunks after a short pause. Both have bounded audio queues; overflow or a provider failure pauses capture with an explicit retry.
- Live transcript deltas revise one draft. The local 1.5-second endpoint commits the provider turn; an interim result is never treated as final. Provider item IDs correlate results even when different turns finish out of order. Focus changes pin the current phrase to its original device. Connections are reused for that device, Agent and configuration revision.
- A live connection failure pauses capture. Explicit retry retains the capture session and accepted chat receipts; unfinished transcript text returns to the editable composer. Unconfirmed audio is not replayed or represented as recognized speech.
- The literal transcript and interpreted instruction remain separate. Audio-chat recognition receives up to four recent active instructions from the same capture session and conversation, within its 12 KB context budget. Superseded receipts are omitted, so another spoken correction references the current revision. An intervening phrase does not discard the earlier instruction from this window. Original messages and executed actions remain in history. Correction branches and foreign-session references are rejected in PostgreSQL.
- The existing durable Outbox and protobuf chat API accept the instruction. Voice bypasses the typed-input quiet window.
- Confirmed microphone activity in live transcription holds pending Agent actions for the same conversation. The final voice receipt releases the hold after commit so the worker can apply the correction. A lost connection or missing receipt expires five seconds after the last audio or commit frame. Device actions already dispatched finish through their existing receipt path.
- New speech stops obsolete playback, closes the synthesis socket and cancels its upstream response. Later text from that interrupted reply remains silent; a new reply can speak. Reply text is synthesized in sentence-sized pieces, with one playback owner. A speech-output failure leaves input available and preserves the sentence for explicit retry; provider failures are not automatically retried.
- Turning Copilot off stops microphone tracks, audio processing, pending recognition and playback. Unsent recognized text is restored to the text composer. Unrecognized buffered audio is discarded; already accepted Agent work continues.

## Ownership and persistence

| Owner | Responsibility |
|---|---|
| `Microphone` / `SpeechSegments` | Capture resources, local voice activity, phrase boundary and device pinning |
| `CopilotSession` | One capture lifetime, per-conversation input cores and bounded recognition jobs |
| `CopilotInput` / existing Outbox | Ordered final text, immutable submission receipts and recoverable drafts |
| Rust `speech` module | Agent configuration, device binding and compatible provider requests |
| Existing Agent runtime | Durable messages, steering, tools, real device evidence and task lifecycle |
| `CopilotOutput` | Sentence offsets, playback interruption and explicit output retry |

The business audio contract is [voice.proto](../../packages/protocol/proto/ai_phone/v1/voice.proto); accepted speech metadata uses [assistant.proto](../../packages/protocol/proto/ai_phone/v1/assistant.proto). REST recognition uses bounded protobuf WAV messages. Live input uses binary protobuf WebSocket frames at `/api/voice/stream`; synthesis uses `/api/voice/playback`, both forwarded by Next.js to Rust. One playback socket carries one synthesis request, bounded WAV chunks and an explicit completion or failure frame. Only the external provider adapter uses provider JSON/base64 or multipart. Raw recordings and provisional recognition results are not archived by AI Phone. Accepted literal/text/correction metadata is private conversation data. Provider-side retention depends on the chosen service.

HTTP clients reuse the existing bounded routing pools. REST configuration and device binding are checked once per request; live sessions check browser origin, the existing session cookie, device binding and configuration revision at connection setup. No database query or extra planning/reflection model call runs for each audio frame. Buffers and pending turns are bounded; closing capture drops its upstream connection. Ownership, action IDs and unknown-result handling remain at the device execution boundary.

## Microphone access

Access is requested only when Copilot is enabled. The macOS bundle supplies a microphone purpose string. Desktop microphone access follows the platform WebView permission flow. For microphone access in a browser, use localhost or a trusted HTTPS origin. The workbench reports unavailable microphones.

Guest voice messages, microphone injection and phone calls are planned separately.

## Qualification and remaining work

Rust and PostgreSQL checks cover audio containers, provider adapters, settings revisions, cancellation and correction chains. Browser capture, device switching, submission and playback interruption still need runtime verification. Configure a speech service and measure silence-to-text, text-to-acceptance, interruption-to-playback-stop, first spoken reply and first device action on a real microphone.

## References

[OpenAI audio APIs](https://developers.openai.com/api/docs/guides/audio), [Realtime transcription](https://developers.openai.com/api/docs/guides/realtime-transcription), [browser microphone requirements](https://developer.mozilla.org/en-US/docs/Web/API/MediaDevices/getUserMedia), [macOS application bundle permissions](https://v2.tauri.app/distribute/macos-application-bundle/).

---

# Copilot 语音

在对话输入区的 Liberty 右侧开启 Copilot，持续说话即可；可与 Liberty 同时开启，语音指令优先。配置位于原有配置中心的 Agent → 语音，识别模型、合成模型、音色和密钥独立于聊天模型，不要求先通过聊天额度探测。

已实现浏览器采集、Rust REST 和实时转写、protobuf 音频、持久纠错和可打断播放。真实麦克风、服务商与端到端延迟待验证。实时转写显示增量草稿，只有最终转录进入聊天。回复按句合成，PCM16 分块播放。

停顿 1.5 秒后结束语句，必须得到完整转录才发送；网络和识别耗时另计。音频对话协议一次完成转录及明确的口误修正；识别上下文从同一采集会话的已接受记录提取，最多保留最近4条、合计12KB的有效指令，排除已被纠正的旧回执，连续补充不会只剩最后一句。普通转写协议提供原话。发送后的修正以新消息关联原回执，同一会话不中断，原话和已执行操作保留。关闭只停采集与播放，已接受任务继续，已识别但未发送的文字回到输入框。音频默认不落盘，错误不自动重复请求。

实时转写显示原话增量。检测到说话后，当前设备对话里尚未派发的动作短暂等待。最终转录提交到数据库后，Agent 先读取新指令。连接断开或没有提交回执时，等待在最后一帧后五秒结束。连接按设备、Agent、配置版本复用，入口校验一次，不逐音频帧查询数据库或新增模型反思。断线后显式重试保持采集session及已接受回执，未确认的文字回到可编辑草稿，不重放未知音频。Rust局域网转发已支持WebSocket连接，但收音仍需要localhost或可信安全来源；局域网入口的真实收音尚未验收。

切换设备不会把说到一半的内容发给另一设备，下一句跟随当前设备。新语音立即停止旧播放并关闭合成连接、取消上游响应；同一旧回复的迟到文字保持静音，新回复可以朗读。合成失败不会堵塞输入，保留原句供显式重试。服务和密钥由用户配置，不自带或自动购买。宿主麦克风不会自动传给手机或电脑客体；语音消息、设备内音频注入及电话能力仍是后续扩展。浏览器收音需要本机地址或可信 HTTPS。
