# Copilot voice

Copilot adds continuous microphone input and interruptible spoken replies to the existing conversation. Enable it beside Free exploration in the conversation composer. Both modes can remain active; spoken instructions have priority. Switching devices preserves a phrase's original destination; the next phrase follows the focused device. Already accepted tasks continue when capture stops or the view changes.

**Status:** browser capture, Agent-owned speech configuration, Rust REST and persistent transcription adapters, protobuf transport, durable corrections and playback are implemented. Isolated PCM, HTTP, WebSocket, database and production Next.js proxy tests cover the transport contracts. Real microphones, speech-provider accuracy, echo suppression, native permission prompts and end-to-end latency still require acceptance. Reply text is synthesized in sentence-sized requests. WAV bytes stream back over protobuf WebSocket frames: PCM16 playback starts before synthesis finishes; other WAV codecs retain browser decoding after download. Native speech-to-speech and WebRTC are not implemented.

## Configure an Agent

Open **Configuration → Agent → Voice**. Enter the compatible API URL, API key, input protocol, recognition model, speech model and voice. The key is editable and can be shown in this form; it is stored in the private database and excluded from ordinary configuration projections. A local compatible service may leave the key empty. Voice configuration has its own revision and does not require a successful chat-model probe. The text/tool model remains independently configured.

| Input protocol | Provider requests | Correction capability |
|---|---|---|
| Live transcription | Persistent `/realtime?intent=transcription` WebSocket with a transcription session | Incremental literal text; only completed turns enter chat. Spoken corrections remain user instructions in the existing continuous conversation. This adapter does not rewrite an earlier instruction semantically. |
| Audio chat | `/chat/completions` with `input_audio` and text output | One request transcribes the phrase and interprets explicit spoken edits. The provider must support this audio-chat contract. |
| Transcription | Multipart `/audio/transcriptions` | Literal transcription; short pauses may update the draft. Semantic spoken-edit interpretation is not supplied by this adapter. |

All input modes use `/audio/speech` with WAV output for replies. These are compatibility contracts, not a claim that every provider or every audio model supports them. Select a model supported by the chosen protocol. Live transcription uses the documented transcription-session schema; it does not accept an arbitrary conversation-only realtime model. Other providers' streaming protocols need their own adapters.

## Continuous conversation

- AudioWorklet captures mono audio, locally resampled to 16 kHz PCM16. REST modes do not make a request per frame. Live transcription streams a confirmed utterance over one persistent connection, including its endpoint silence, instead of uploading a complete WAV after endpointing. Rust converts to the provider's 24 kHz PCM format while retaining interpolation state across frames.
- **1,500 ms of microphone silence** seals a phrase. Only final recognized text is committed. Recognition/network time is additional; the UI does not promise delivery 1.5 seconds after speech regardless of the provider.
- Audio-chat interpretation runs once per completed phrase, with a hard 28-second chunk boundary. Transcription mode can send phrase chunks after a short pause. Both have bounded audio queues; overflow or a provider failure pauses capture with an explicit retry.
- Live transcript deltas revise one draft. The local 1.5-second endpoint commits the provider turn; an interim result is never treated as final. Provider item IDs correlate results even when different turns finish out of order. Focus changes pin the current phrase to its original device. Connections are reused for that device, Agent and configuration revision.
- A live connection failure pauses capture. Explicit retry retains the capture session and accepted chat receipts; unfinished transcript text returns to the editable composer. Unconfirmed audio is not replayed or represented as recognized speech.
- The literal transcript and interpreted instruction remain separate. An explicit correction can link to the latest accepted instruction in the same capture session and conversation. Original messages and executed actions remain in history. Correction branches and foreign-session references are rejected in PostgreSQL.
- The existing durable Outbox and protobuf chat API accept the instruction. Voice bypasses the additional typed-input quiet window; it does not create a second Agent loop or conversation store.
- New speech stops obsolete playback, closes the synthesis socket and cancels its upstream response. Reply text is synthesized in sentence-sized pieces, with one playback owner. A speech-output failure leaves input available; provider failures are not automatically retried.
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

The business audio contract is [voice.proto](../../packages/protocol/proto/ai_phone/v1/voice.proto); accepted speech metadata uses [assistant.proto](../../packages/protocol/proto/ai_phone/v1/assistant.proto). REST recognition uses bounded protobuf WAV messages. Live input uses binary protobuf WebSocket frames at `/api/voice/stream`; synthesis uses `/api/voice/playback`, both forwarded by Next.js to Rust. One playback socket carries one synthesis request, bounded WAV chunks and an explicit completion or failure frame. There is no second speech settings store. The retired buffered `/api/voice/speech` operation and `VoiceAudio` response have no remaining product consumers. Only the external provider adapter uses provider JSON/base64 or multipart. Raw recordings and provisional recognition results are not archived by AI Phone. Accepted literal/text/correction metadata is private conversation data. Provider-side retention depends on the chosen service.

HTTP clients reuse the existing bounded routing pools. REST configuration and device binding are checked once per request; live sessions check browser origin, the existing session cookie, device binding and configuration revision at connection setup. No database query or extra planning/reflection model call runs for each audio frame. Buffers and pending turns are bounded; closing capture drops its upstream connection. Ownership, action IDs and unknown-result handling remain at the device execution boundary.

## Microphone access

Access is requested only when Copilot is enabled. The macOS bundle supplies a microphone purpose string; Android declares recording permission and uses the existing WebView permission flow. AI Phone requests audio, not camera or location. A browser needs a secure context: use localhost or a trusted HTTPS origin. A raw HTTP LAN address must not be assumed to support microphone capture. The application reports an unavailable microphone rather than silently switching to text.

The native workbench has no additional arbitrary host-command capability for voice. Microphone input is not implicitly broadcast into either guest device. Sending voice messages, injecting audio into a guest microphone and placing calls are separate future device capabilities.

## Qualification and remaining work

Local tests exercise real WAV containers, compatible HTTP bodies, quota failure without retry, final-only dispatch, repeated corrections, bounded retention, focus changes during a phrase, durable Outbox handoff, output failure independent of capture, interruption and late cancellation. Disposable PostgreSQL tests cover independent voice settings, revision conflicts, correction chains and permission/restore recovery. Synthetic PCM and a mock provider do **not** prove real speech quality or microphone playback.

Acceptance still needs a configured speech service and a real microphone: multiple languages, quoted negation, repeated self-corrections, background noise, echo, long sessions, permission denial and device switching. Measure capture-to-final-text, silence-to-durable-acceptance, barge-in-to-playback-stop, first spoken reply and first real device action separately, including p50/p95 and network time. No measured end-to-end speed claim is made yet.

Automatic audio-session replay/resume, native speech-to-speech, guest voice messages and explicit test-call support remain follow-up capabilities. Speech services and keys are supplied by the user; none is bundled or automatically purchased. No real speech-quality or latency acceptance is claimed by the synthetic transport tests. Live transcription currently targets the local/client ingress; the legacy plain-HTTP LAN forwarding script does not forward WebSocket upgrades and is not a qualified voice ingress.

## References

[OpenAI audio APIs](https://developers.openai.com/api/docs/guides/audio), [Realtime transcription](https://developers.openai.com/api/docs/guides/realtime-transcription), [browser microphone requirements](https://developer.mozilla.org/en-US/docs/Web/API/MediaDevices/getUserMedia), [macOS application bundle permissions](https://v2.tauri.app/distribute/macos-application-bundle/).

---

# Copilot 语音

在对话输入区的自由探索右侧开启 Copilot，持续说话即可；可与自由探索同时开启，语音指令优先。配置位于原有配置中心的 Agent → 语音，识别模型、合成模型、音色和密钥独立于聊天模型，不要求先通过聊天额度探测。

当前已有浏览器采集代码、Rust REST及持续实时转写、protobuf 音频、持久纠错和可打断播放，隔离 PCM、HTTP、WebSocket、Outbox、数据库和生产 Next.js 转发合同有测试。**真实麦克风、服务商识别质量、回声、原生权限提示及端到端延迟仍未验收。** 实时转写在封句前传输音频并显示增量草稿，1.5秒停顿后提交片段，只有服务端最终转录进入聊天。回复按句调用原有WAV合成服务，分块经protobuf WebSocket传回；PCM16收到即排入播放，其他WAV编码保留完整解码。没有实现原生语音对语音或WebRTC。

停顿 1.5 秒后结束语句，必须得到完整转录才发送；网络和识别耗时另计。音频对话协议一次完成转录及明确的口误修正；普通转写协议仅提供原话，不承诺理解“说错了”。发送后的修正以新消息关联原回执，同一会话不中断，原话和已执行操作保留。关闭只停采集与播放，已接受任务继续，已识别但未发送的文字回到输入框。音频默认不落盘，错误不自动重复请求。

实时转写协议只提供原话增量，不承诺语义改写旧指令；新说的纠正进入原连续会话。连接按设备、Agent、配置版本复用，入口校验一次，不逐音频帧查询数据库或新增模型反思。断线后显式重试保持采集session及已接受回执，未确认的文字回到可编辑草稿，不重放未知音频。当前本机/客户端入口有转发资格化，旧明文HTTP局域网脚本不支持WebSocket升级，不能当作已验收语音入口。

切换设备不会把说到一半的内容发给另一设备，下一句跟随当前设备。新语音立即停止旧播放并关闭合成连接、取消上游响应；合成失败不会堵塞输入。服务和密钥由用户配置，不自带或自动购买。宿主麦克风不会自动传给手机或电脑客体；语音消息、设备内音频注入及电话能力仍是后续扩展。浏览器需本机地址或可信 HTTPS；普通局域网 HTTP 不保证麦克风可用。
