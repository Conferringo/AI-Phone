# Copilot voice — implementation plan

Status: **planned; no voice mode is shipped yet**. This contract extends the existing [continuous conversation and steering](conversation-steering.md). It does not replace the model adapter, device action ledger or conversation history.

## Experience

Copilot has one toggle immediately to the right of Free exploration in each conversation composer. It keeps listening until switched off; it does not require holding a button. A compact listening indicator and live transcript replace explanatory banners. Manual typing keeps its existing Enter/newline and Cmd/Ctrl+Enter/send behavior. Microphone denial, provider errors and a disconnected transport remain visible and retryable.

Copilot and Free exploration can coexist; this is the selected behavior. Spoken instructions have priority at the next safe execution boundary. Only one composer owns microphone capture at a time. Switching focus preserves the bound conversation; moving microphone ownership to another conversation is explicit and visible. A connection between two devices does not merge their conversations.

1. Voice activity updates a provisional transcript in the composer. Partial hypotheses revise that draft instead of creating a new message for every word.
2. After **1,500 ms without speech**, commit one user message and return immediately to listening. Continued speech produces another message in the **same conversation**, steering an active run where possible. The endpoint timer uses audio activity, not the arrival time of a delayed transcript packet. Commit the latest usable transcript at that deadline while requesting finalization; a late final result becomes an explicit revision/correction, not a reset of the session. If no text is available, show recognition pending/error rather than inventing a message.
3. Before commit, an explicit correction such as “no, I meant the other file” revises the draft while preserving the actual instruction. Quoted words and ordinary negation must not be mistaken for corrections. Recognition uncertainty stays editable; do not silently invent missing words.
4. After commit, a correction is a new linked instruction with its own receipt. The conversation shows what was accepted and its correction. Already executed device actions remain in history. A pending plan may change; an uncertain send or click is observed before any retry.
5. Agent feedback begins with a short useful acknowledgement when work takes time, then streams text and speech. New user speech stops obsolete playback immediately and steers subsequent reasoning. Listening remains active while the Agent operates a device.
6. Switching Copilot off stops capture, endpoint timers and playback, closes the provider session, and discards unsent audio. Keep an unsent text draft. Already accepted work continues under the existing task controls; disabling a microphone is not canceling a task.

## Runtime ownership

| Owner | Responsibility |
|---|---|
| Composer / capture session | Microphone, voice activity, echo handling, local draft revision, the 1.5-second timer and listening state |
| Voice gateway | Negotiated audio formats, ordered streaming provider events, reconnect/resume, backpressure and credentials kept server-side |
| Agent runtime | Durable accepted messages, priority steering, context compression, tasks, tools and device evidence |
| Playback session | Synthesized audio queue, output device, interruption generation and bounded latency |
| Guest media adapter | Separately routed phone/computer microphone input and device playback; never implicit host-microphone broadcast |

Voice connection, task execution and device control are independent state machines. A transcription stream reconnect does not create a new conversation or cancel a device action. Existing PostgreSQL messages/steering stay the durable source of truth. Do not add a second competing conversation store. Interim audio and hypotheses remain in bounded memory; raw recordings are not saved by default.

Initial capture states are `off → connecting → listening ↔ speaking → committing → listening`, with a recoverable `error` state. Listening may continue while a previous commit is being acknowledged: use distinct utterance IDs rather than blocking the capture loop. Playback has its own generation; late chunks from an interrupted generation are ignored.

Recognition revisions and semantic corrections are different. The reducer keeps the current literal hypothesis and intended draft separately. Prefer a voice adapter that can emit both in the same stream. A pipeline adapter may run a correction pass on stable utterances asynchronously, with a strict deadline; never add a model request for every audio chunk or block the 1.5-second commit waiting for it. Late intent corrections become linked steering events. Quoted negation, uncertain references and multiple self-corrections are required qualification cases. Provider choice must be measured against this complete path.

## Message contract, before protobuf implementation

The following are domain fields, **not a new API already implemented**:

- Bind a session once to authenticated user, Agent and conversation/device. Give each utterance an ID, each stream event an increasing sequence and each transcript an explicit revision/final flag. Negotiation pins provider/model and supported formats for that session.
- Draft patches identify the utterance and previous revision. A stale or reordered patch cannot overwrite a newer draft. An end-of-speech event carries the last audio timestamp, allowing exact endpoint measurement.
- A commit has a stable existing chat `request_id`, final text, the utterance identity and an optional correction target. Acknowledgement distinguishes accepted, consumed by the run and completed; UI does not claim an action at transcription time.
- Retries reuse the same commit ID and exact body. Reconnect asks for the last acknowledged sequence and accepted IDs. If provider audio resumption is unavailable, retain the accepted transcript and begin a new segment; never fabricate that missing speech was recognized.
- Bound the audio backlog and the unacknowledged commit queue. If either fills, surface the failure and preserve available text. Do not silently drop speech or resend it with a new ID. Existing steering queue limits and the current 700 ms text-burst quiet window must be reconciled explicitly before continuous-input release; do not stack that debounce after the 1.5-second voice endpoint.
- Before each model context compaction, retain the current task, recent corrections, pending inputs, action outcomes and unresolved operations. Compression does not clear the visible transcript or change a session ID. Verify that multiple interruptions and a reconnect preserve accepted order.

New business messages must be defined in `packages/protocol/proto`, generated for Rust/TypeScript, and streamed as protobuf WebSocket frames. Audio is a separately negotiated media stream, not JSON/base64 added to every chat request. The first implementation reuses existing chat submission and steering rather than adding a parallel Agent loop.

## Models and setup

Speech is configured within the existing Agent form, not in a second settings center. Its text model can stay unchanged. Support both a streaming STT → existing Agent → streaming TTS pipeline and a provider-native realtime adapter through one capability contract. A text/tool model is not assumed to accept audio.

Probe input audio, partial/final transcription, output audio, interruption and session-resume capabilities independently. Show only options the selected adapter actually supports. Missing speech configuration must produce a concrete error; do not present a working Copilot toggle that silently falls back to text-only behavior. Provider implementations and actual account qualification remain pending.

The installed client needs native microphone integration. Localhost and trusted-LAN browser capture need separate acceptance; an ordinary insecure LAN URL cannot be assumed to permit microphone capture. Request microphone access when the user enables Copilot, never at app startup. Internet access is only used for the configured speech/model service; no public callback or tunnel is required.

## Latency budget and verification

These are **engineering targets**, not measured results or promises about cloud providers:

| Measurement | Initial target |
|---|---|
| Local speech-activity indicator | ≤100 ms from captured audio |
| First provisional transcript | p95 ≤500 ms when the provider supports streaming partials |
| Silence to commit dispatch | 1,500 ms plus ≤100 ms local scheduling; no extra debounce before chat dispatch |
| Accepted correction to running-model cancellation signal | p95 ≤150 ms on the local service |
| Speech onset to obsolete-playback stop | p95 ≤120 ms locally |
| Committed input to first spoken feedback | p95 ≤1,200 ms on a qualified provider/network, measured separately from endpoint delay |

Instrument capture, audio upload, first partial, final transcript, commit dispatch, durable acceptance, steering consumption, first output audio and first device action with monotonic timestamps. Report cold/warm sessions and p50/p95, including queue time. Do not claim a speedup from fewer lines of validation without end-to-end measurements.

Authenticate and negotiate once at the stream boundary; do not rehash installer components, read all settings, write a database row, run a second model or perform device observation for every audio frame. Cache configuration by revision and reuse provider connections. Grounding and action identity checks remain at logical device actions. Interruption cancels obsolete model/voice work without replaying side effects. Release acceptance compares the real voice-and-action path, not isolated token speed.

## Delivery slices

1. **Text-correction contract:** deterministic recorded transcript events, timer boundaries, late final segments, duplicates, “not/no” ambiguity, multiple corrections and reconnect. Integrate with existing steering in an isolated conversation.
2. **Continuous input:** one actual streaming STT adapter, capture lifecycle on native/browser, 1.5-second commits, bounded queues and real microphone testing. Keep user devices/accounts out of automated fixtures.
3. **Spoken feedback:** actual streaming output, echo suppression, barge-in and playback generations; test text+voice and provider errors during tool execution.
4. **Guest speech:** explicit output routing to a phone/computer virtual microphone, followed by real app voice-message tests. Host playback, guest microphone and guest audio must be independently selectable and avoid feedback loops.
5. **Calls:** an app-call capability adapter and evidence for connect/mute/hangup; carrier/SIP integration is a separate extension. Root, an emulator or a changed device name does not supply a phone number or eSIM. Test calls only with explicitly designated test endpoints.

Do not couple the current offline-distribution completion gate to speculative calling code. Publish support only after the corresponding slice has real evidence.

---

# Copilot 实时语音开发契约

**当前为版本计划，语音功能尚未实现。** 入口放在自由探索右侧，开启后持续监听，1.5秒没有语音就提交；后续补充保持同一会话，并优先调整当前任务。暂按与自主探索可并行设计，用户语音优先；一个麦克风同时只归属一个明确的对话。

识别中的文字在输入区修订，避免每个字都变成一条消息。“不是、说错了”等明确纠错在发送前修订草稿；普通否定或引用不能误删。发送后的纠错作为有回执的新指令进入现有steering，不抹掉已接受的消息和真实动作。停用只关闭语音采集与播放，保留文字草稿，已经接受的任务继续遵循原有控制。

实时采集、语音网关、Agent任务、播放和客体麦克风分别负责自己的生命周期。连接、断线、重连不重建聊天；音频默认不落盘。临时识别不逐帧写数据库、不反复读取配置、不重做安装资源校验。鉴权和格式协商在会话入口完成；设备副作用仍在执行边界保留action_id、控制epoch与真实观察。响应速度用音频采集→识别→提交→执行/语音反馈的p50/p95衡量，表中时间均为待验收目标。

模型配置归入原有Agent表单，允许语音识别＋现有文字模型＋语音合成，以及原生实时语音两类适配。不把普通文字模型当成语音模型；输入/输出/打断/重连能力分别探测。后续协议先改protobuf，音频独立协商传输。多个补充、晚到识别、排队上限、上下文压缩和重连必须一起验证，不能靠重新开启session丢掉上下文。

开发顺序：先验证纠错与1.5秒端点的事件契约，再接真实麦克风和流式识别，再做可打断语音反馈，最后接客体虚拟麦克风、语音消息与测试通话。前面的文字、语音与任务能力先可用，不等电话能力。每一阶段都要有真实证据；当前不增加未接通的按钮。
