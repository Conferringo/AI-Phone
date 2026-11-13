# Technical architecture

[简体中文](architecture.zh-CN.md)

Next.js App Router/Turbopack, React, Tailwind, shadcn/ui and GSAP provide the workbench. Rust/Axum/Tokio own durable runs, model adapters, tools, permissions and completion auditing. PostgreSQL and SQLx store registry, conversations, WhatsApp linked-device sessions, and the task queue. Redis wakes the connected-app delivery process after a committed task result. PostgreSQL remains the source for the result. Cross-device handoffs wake their scheduler immediately after commit. [Kafka export](event-relay.md) streams committed handoff receipts for external consumers.

Android runs natively through Android Emulator, controlled by a Rust runner, ADB and guest bridges. Native UI targets with OCR/screenshot fallback ground actions. The Rust media runner uses Android gRPC and the Rust guest capture service for JPEG frames and PCM audio. Native touch indicators/pointers are captured in the same frame. The Linux computer uses Docker, Debian/Openbox and Xvfb. A native Rust guest service owns X11 capture and input, cursor receipts, audio-process supervision, capture leases, durable notifications and Chromium location sessions. Its location CLI and loopback daemon use protobuf and preserve the existing private state file. FFmpeg provides screen/audio encoding. The workbench supports screenshot-based control and read-only regional zoom.

The phone Runner and native boot observer share a [Rust ADB client](engineering/device-bridge.md#runner-adb-transport). Runner commands no longer spawn an ADB executable; the host SDK still provides the ADB server, and APK setup paths retain SDK tool dependencies.

Text and Markdown files open in Mousepad by default. The desktop includes file-type detection; [personal MIME associations](https://specifications.freedesktop.org/mime-apps/latest/file.html) take precedence over image defaults.

Device audio uses stereo 48 kHz signed 16-bit PCM, with independent playback streams across device focus changes. The desktop's PulseAudio monitor uses a 50 ms clock window without rewinds; the Rust capture lease owns the encoder and releases it when its connection closes.

Business APIs use protobuf (`packages/protocol/proto`). Browser requests use `application/x-protobuf`; streaming business events use protobuf WebSocket frames. Media and health checks are separate. Model SSE is a separate transport.

The service-only Android helper supplies live accessibility hierarchies through a read-only request protected by Android's `DUMP` permission. Rust accepts a complete reply bound to the current request ID before trying UIAutomator and OCR. Password text is excluded from the hierarchy. Actions still require a control lease and post-action verification.

The phone and desktop share a bounded Rust OCR pipeline for PNG decoding, screen signatures and native Tesseract TSV parsing. Tesseract runs in a network-isolated, read-only service that shares the workspace image, or through the verified Linux offline dependency bundle. Agent task tools remain separate from this service. `computer_find_text` locates labels in the latest authorized screenshot and returns full-screen bounds, confidence and the source observation ID. Repeated labels are returned as separate OCR candidates. Recognition runs on demand, within the existing five-second budget. An optional pixel region enlarges small labels; detected bounds are projected back to the original screenshot coordinates.

## Execution

User/paired channel → durable run → pinned Agent/model revision → Skills/current observation → proposed tool → owner/device/run/epoch/action-ID checks → guest execution → post-action observation → persisted result → next round → independent completion audit.

Completion uses post-action observations or delivery receipts. Observe unknown outcomes before retrying. The UI shows progress, tool inputs and results, and completion audits.

Task attempts progress independently on different devices. PostgreSQL grants exclusive device leases during admission; an attempt retains its primary and borrowed devices until its result is committed. Model and guest I/O hold no SQL transaction. Cancellation targets a run, while foreground device changes leave other runs active. Cross-device control waits for an occupied device instead of taking another task's ownership; device messages provide durable handoff. Startup releases the previous process's leases while preserving checkpoints, unknown actions and wait deadlines.

An occupied device returns `device_busy` before dispatching input, with a wait or handoff recommendation. The existing task retains control. Other errors retain their execution-status contract; unknown outcomes still require observation.

Blocked attempts and successful waits do not consume the execution-failure budget or count as execution progress. Actual failures retain the same threshold, including invalid wait and plan requests.

Responses, Chat Completions and native Anthropic Messages use the same engine. The latter translates tools, results and images; truncated, filtered, unterminated or malformed tool outputs are rejected. Providers must pass actual text, image and tool round-trip probes.

A Responses completion event with an explicit non-completed response status is rejected before its output can become tool proposals. Complete output is preserved verbatim for the existing evidence and provider-replay paths.

## Visible execution and latency

Ordinary device tasks use phone-use or computer-use. Online research runs in the device browser; application searches stay in that application. Command/fetch tools require an explicit programming or maintenance purpose; a request does not gain them because device or search keywords were absent. Research about programming does not authorize command-based fetching. The catalog contains registered functions only, without provider-native search tools, and dispatch rechecks the persisted request so queued legacy calls cannot bypass the policy. Lookup completion requires a successful visible interaction and a subsequent real device observation; browser research also requires browser-frame metadata.

The engine obtains the first authorized observation before asking the model to plan, and reuses post-action frames. Model attempts record preparation, response headers, first stream byte and total transport timings, including failed and interrupted calls. Protobuf carries these values in the execution trace alongside tool and completion-check durations. Trace reads are scoped to the requested message.

Repeated model turns reuse HTTP connection pools by proxy route. Each request supplies its own credentials; routes, cancellation and per-request timeouts retain their boundaries. At most eight route pools are retained per service instance. This removes repeated connection setup without lowering model reasoning settings or skipping execution checks.

## Skill storage

PostgreSQL owns committed Skill text. Each installation serializes its file projection with a per-Skill operating-system lock, then atomically publishes `SKILL.md` and synchronizes it to disk. The blocking writer retains the lock until file operations finish, including after cancellation of its async caller. File I/O holds no database transaction; reads and startup reconcile the projection from durable text while preserving auxiliary resources.

## Devices and exchange

The default is one phone and one computer. Root and sudo apply inside the guests. A persistent device link gates file exchange, with device/direction/path/hash checks; disconnecting rejects new transfers. Removing an existing link checks its persisted ownership without requiring either runtime to be online. Reconnecting still requires ready runtimes. Both devices share one conversation, one draft and one selected Agent. Each run keeps its device identity.

Development checks verified a phone-to-computer file round trip, durable action retention and isolated worker/database recovery. Real-account WeChat delivery, host sleep/reboot, prolonged multi-device use and notification/task contention remain open. See [task recovery](task-recovery.md) and [release status](release-status.md).

Managed staging separates versioned source and persistent user state. The development workspace keeps its existing data directory. See [distribution and updates](distribution.en.md).
