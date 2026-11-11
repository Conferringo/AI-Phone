# Copilot voice

Enable Copilot beside Liberty in the conversation composer. A background waveform shows when Copilot is listening, hearing speech, or reading a reply. You can keep both modes on. Voice instructions have priority.

## Desktop speech

The macOS and Windows clients bundle a 32 MB multilingual Whisper model. Turn on Copilot and allow microphone access. The client updates the draft about twice per second, closes the phrase after 650 ms of silence, and submits the final text to the same Agent conversation. The system voice reads replies. Speaking again interrupts playback. Desktop speech uses the installed model and system voice without a speech API key.

The client sends 16 kHz mono PCM to its Rust host over binary protobuf IPC. The host loads the model once per Copilot session and decodes locally. The Agent receives the final transcript, interprets corrections, and runs the usual tools.

## Browser speech

Copilot uses an installed on-device recognition pack where the browser supports it. If a pack is required, the workbench offers **Install language pack**. The selected interface language sets the recognition language and local voice. Browser recognition revises the draft as you speak and submits the final transcript when it detects a pause. Listening resumes for the next phrase in the same conversation. Start speaking to interrupt playback. A phrase follows the device it started on. The next phrase uses the currently focused device.

Installed system voices read replies. Browsers without an on-device recognition pack can use configured Agent speech input.

## Configure a speech provider

Open **Configuration → Agent → Voice** and enter a compatible API URL, key, input protocol, and recognition model. A local compatible service can leave the key empty. The remote speech model and voice under **Advanced** are optional and used together when provider input is active and a matching system voice is unavailable.

| Input protocol | Provider requests | Correction capability |
|---|---|---|
| Live transcription | Persistent `/realtime?intent=transcription` WebSocket with a transcription session | Incremental literal text; only completed turns enter chat. Spoken corrections remain user instructions in the existing continuous conversation. |
| Audio chat | `/chat/completions` with `input_audio` and text output | One request transcribes the phrase and interprets explicit spoken edits. The provider must support this audio-chat contract. |
| Transcription | Multipart `/audio/transcriptions` | Literal transcription with draft updates at short pauses. |

Remote playback uses `/audio/speech` with WAV output. Live transcription requires a model that supports the transcription-session schema.

## Provider input and playback

- AudioWorklet captures mono audio, locally resampled to 16 kHz PCM16. REST modes do not make a request per frame. Live transcription streams a confirmed utterance over one persistent connection, including its endpoint silence, instead of uploading a complete WAV after endpointing. Rust converts to the provider's 24 kHz PCM format while retaining interpolation state across frames.
- **1,500 ms of microphone silence** seals a provider phrase. Final recognized text is then sent to the existing chat outbox.
- Audio-chat interpretation runs once per completed phrase, with a hard 28-second chunk boundary. Transcription mode can send phrase chunks after a short pause. Both have bounded audio queues; overflow or a provider failure pauses capture with an explicit retry.
- Live transcript deltas revise one draft. The local 1.5-second endpoint commits the provider turn; an interim result is never treated as final. Provider item IDs correlate results even when different turns finish out of order. Focus changes pin the current phrase to its original device. Connections are reused for that device, Agent and configuration revision.
- A live connection failure pauses capture. Explicit retry retains the capture session and accepted chat receipts; unfinished transcript text returns to the editable composer. Unconfirmed audio is not replayed or represented as recognized speech.
- The literal transcript and interpreted instruction remain separate. Audio-chat recognition receives up to four recent active instructions from the same capture session and conversation, within its 12 KB context budget. Superseded receipts are omitted, so another spoken correction references the current revision. An intervening phrase does not discard the earlier instruction from this window. Original messages and executed actions remain in history. Correction branches and foreign-session references are rejected in PostgreSQL.
- The existing durable Outbox and protobuf chat API accept the instruction. Voice bypasses the typed-input quiet window.
- New speech stops obsolete playback, closes the synthesis socket and cancels its upstream response. Later text from that interrupted reply remains silent; a new reply can speak. Reply text is synthesized in sentence-sized pieces, with one playback owner. A speech-output failure leaves input available and preserves the sentence for explicit retry; provider failures are not automatically retried.
- Turning Copilot off stops microphone tracks, audio processing, pending recognition and playback. Unsent recognized text is restored to the text composer. Unrecognized buffered audio is discarded; already accepted Agent work continues.

## Ownership and persistence

| Owner | Responsibility |
|---|---|
| `Microphone` / `SpeechSegments` | Capture resources, local voice activity, phrase boundary and device pinning |
| `NativeRecognition` / `LocalSpeech` | Progressive transcription and one offline decoder per desktop Copilot session |
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

## References

[Web Speech API](https://webaudio.github.io/web-speech-api/), [OpenAI audio APIs](https://developers.openai.com/api/docs/guides/audio), [Realtime transcription](https://developers.openai.com/api/docs/guides/realtime-transcription), [macOS application bundle permissions](https://v2.tauri.app/distribute/macos-application-bundle/).

---

# Copilot 语音

在输入区开启 Copilot，可同时开启 Liberty，语音指令优先。工作台底层声波显示等待、收音和播报状态。macOS 和 Windows 客户端随包提供约 32 MB 的多语 Whisper 模型，播报使用系统音色。开启麦克风即可使用，无需填写语音 API。

客户端约每半秒更新草稿，静音 650 毫秒后完成断句，最终文字立即进入原会话。说下一句会打断播报，Agent 继续理解补充和修正。音频在宿主端识别，聊天 Agent 接收最终文字并执行原有工具。

浏览器支持本机识别时，Copilot 使用界面语言对应的本机语言包。需要下载时，在工作台提示中点击**安装本机语言包**。说话时显示草稿，识别器判定停顿并返回最终文本后立即发送。说下一句可以打断播报，原有对话继续。正在说的一句发给原设备，下一句发给当前设备。关闭后，未发送的转写回到输入框。

浏览器也可通过**配置中心 → Agent → 语音**使用已有服务，服务输入仍以 1.5 秒静音断句。服务输入下缺少本机音色时，可使用配置的远程合成模型和音色。浏览器收音需要本机地址或可信 HTTPS。
