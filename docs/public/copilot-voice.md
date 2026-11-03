# Copilot voice

Copilot adds continuous microphone input and interruptible spoken replies to the existing conversation. Enable it beside Free exploration in the conversation composer. Both modes can remain active; spoken instructions have priority. Switching devices preserves a phrase's original destination; the next phrase follows the focused device. Already accepted tasks continue when capture stops or the view changes.

**Status:** browser capture, Agent-owned speech configuration, the Rust audio adapter, protobuf transport, durable corrections and playback are implemented. The complete path has isolated PCM/HTTP/database tests. Real microphones, speech-provider accuracy, echo suppression, native permission prompts and end-to-end latency still require acceptance. This is a bounded REST audio implementation, not a provider-native Realtime/WebRTC session.

## Configure an Agent

Open **Configuration → Agent → Voice**. Enter the compatible API URL, API key, input protocol, recognition model, speech model and voice. The key is editable and can be shown in this form; it is stored in the private database and excluded from ordinary configuration projections. A local compatible service may leave the key empty. Voice configuration has its own revision and does not require a successful chat-model probe. The text/tool model remains independently configured.

| Input protocol | Provider requests | Correction capability |
|---|---|---|
| Audio chat | `/chat/completions` with `input_audio` and text output | One request transcribes the phrase and interprets explicit spoken edits. The provider must support this audio-chat contract. |
| Transcription | Multipart `/audio/transcriptions` | Literal transcription; short pauses may update the draft. Semantic spoken-edit interpretation is not supplied by this adapter. |

Both use `/audio/speech` with WAV output for replies. These are compatibility contracts, not a claim that every provider or every audio model supports them. Model IDs and voices come from the selected service. Native Realtime providers need a separate adapter; do not enter their session-only model IDs into this REST configuration.

## Continuous conversation

- AudioWorklet captures mono audio, locally resampled to 16 kHz PCM16. No provider request is made for silence or each audio frame.
- **1,500 ms of microphone silence** seals a phrase. Only final recognized text is committed. Recognition/network time is additional; the UI does not promise delivery 1.5 seconds after speech regardless of the provider.
- Audio-chat interpretation runs once per completed phrase, with a hard 28-second chunk boundary. Transcription mode can send phrase chunks after a short pause. Both have bounded audio queues; overflow or a provider failure pauses capture with an explicit retry.
- The literal transcript and interpreted instruction remain separate. An explicit correction can link to the latest accepted instruction in the same capture session and conversation. Original messages and executed actions remain in history. Correction branches and foreign-session references are rejected in PostgreSQL.
- The existing durable Outbox and protobuf chat API accept the instruction. Voice bypasses the additional typed-input quiet window; it does not create a second Agent loop or conversation store.
- New speech stops obsolete playback. Reply text is synthesized in sentence-sized pieces, with one playback owner. A speech-output failure leaves input available; provider failures are not automatically retried.
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

The business audio contract is [voice.proto](../../packages/protocol/proto/ai_phone/v1/voice.proto); accepted speech metadata uses [assistant.proto](../../packages/protocol/proto/ai_phone/v1/assistant.proto). Browser requests and responses are protobuf, with bounded WAV bytes. Only the external provider adapter uses its provider's multipart/JSON format. Raw recordings and provisional recognition results are not archived by AI Phone. Accepted literal/text/correction metadata is private conversation data. Provider-side retention depends on the chosen service.

HTTP clients reuse the existing bounded routing pools. Configuration and device binding are checked once per audio request, not per frame. There is no extra planning/reflection model call before transcription or every device action. Ownership, action IDs and unknown-result handling remain at the device execution boundary.

## Microphone access

Access is requested only when Copilot is enabled. The macOS bundle supplies a microphone purpose string; Android declares recording permission and uses the existing WebView permission flow. AI Phone requests audio, not camera or location. A browser needs a secure context: use localhost or a trusted HTTPS origin. A raw HTTP LAN address must not be assumed to support microphone capture. The application reports an unavailable microphone rather than silently switching to text.

The native workbench has no additional arbitrary host-command capability for voice. Microphone input is not implicitly broadcast into either guest device. Sending voice messages, injecting audio into a guest microphone and placing calls are separate future device capabilities.

## Qualification and remaining work

Local tests exercise real WAV containers, compatible HTTP bodies, quota failure without retry, final-only dispatch, repeated corrections, bounded retention, focus changes during a phrase, durable Outbox handoff, output failure independent of capture, interruption and late cancellation. Disposable PostgreSQL tests cover independent voice settings, revision conflicts, correction chains and permission/restore recovery. Synthetic PCM and a mock provider do **not** prove real speech quality or microphone playback.

Acceptance still needs a configured speech service and a real microphone: multiple languages, quoted negation, repeated self-corrections, background noise, echo, long sessions, permission denial and device switching. Measure capture-to-final-text, silence-to-durable-acceptance, barge-in-to-playback-stop, first spoken reply and first real device action separately, including p50/p95 and network time. No measured end-to-end speed claim is made yet.

A provider-native streaming adapter, reconnect/resume, guest voice messages and explicit test-call support are follow-up capabilities. None is represented as shipped by this implementation.

## References

[OpenAI audio APIs](https://developers.openai.com/api/docs/guides/audio), [Realtime transcription](https://developers.openai.com/api/docs/guides/realtime-transcription), [browser microphone requirements](https://developer.mozilla.org/en-US/docs/Web/API/MediaDevices/getUserMedia), [macOS application bundle permissions](https://v2.tauri.app/distribute/macos-application-bundle/).

---

# Copilot 语音

在对话输入区的自由探索右侧开启 Copilot，持续说话即可；可与自由探索同时开启，语音指令优先。配置位于原有配置中心的 Agent → 语音，识别模型、合成模型、音色和密钥独立于聊天模型，不要求先通过聊天额度探测。

当前已接通真实浏览器采集代码、Rust 兼容语音接口、protobuf 音频、持久纠错和可打断播放，并通过隔离 PCM、HTTP、Outbox、数据库链路验证。**真实麦克风、服务商识别质量、回声、原生权限提示及端到端延迟仍未验收。** 当前是有界 REST 语音链，不是原生 Realtime/WebRTC。

停顿 1.5 秒后结束语句，必须得到完整转录才发送；网络和识别耗时另计。音频对话协议一次完成转录及明确的口误修正；普通转写协议仅提供原话，不承诺理解“说错了”。发送后的修正以新消息关联原回执，同一会话不中断，原话和已执行操作保留。关闭只停采集与播放，已接受任务继续，已识别但未发送的文字回到输入框。音频默认不落盘，错误不自动重复请求。

切换设备不会把说到一半的内容发给另一设备，下一句跟随当前设备。新语音打断旧播放；合成失败不会堵塞输入。宿主麦克风不会自动传给手机或电脑客体；语音消息、设备内音频注入及电话能力仍是后续扩展。浏览器需本机地址或可信 HTTPS；普通局域网 HTTP 不保证麦克风可用。
