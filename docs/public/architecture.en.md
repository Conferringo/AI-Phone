# Technical architecture

[简体中文](architecture.zh-CN.md)

Next.js App Router/Turbopack, React, Tailwind, shadcn/ui and GSAP provide the workbench. Rust/Axum/Tokio own durable runs, model adapters, tools, permissions and completion auditing. PostgreSQL/SQLx store registry and conversation state; Redis is declared but is not the task queue.

Android runs natively through Android Emulator, controlled by a Rust runner, ADB and guest bridges. Native UI targets with OCR/screenshot fallback ground actions. Bun bridges use Android gRPC and FFmpeg/X11 for JPEG frames and PCM audio. Native touch indicators/pointers are captured in the same frame. The Linux computer uses Docker, Debian/Openbox and Xvfb. A native Rust guest service owns X11 capture and input, cursor receipts, audio-process supervision, capture leases and durable notifications. FFmpeg provides screen/audio encoding. The workbench supports screenshot-based control and read-only regional zoom.

Business APIs use protobuf (`packages/protocol/proto`). Browser requests use `application/x-protobuf`; streaming business events use protobuf WebSocket frames. Media and health checks are separate. Model SSE is a separate transport.

The service-only Android helper supplies live accessibility hierarchies through a read-only request protected by Android's `DUMP` permission. Rust accepts a complete reply bound to the current request ID before trying UIAutomator and OCR. Password text is excluded from the hierarchy. Observation and action validation remain separate: a fast hierarchy does not grant control or replace post-action verification.

## Execution

User/paired channel → durable run → pinned Agent/model revision → Skills/current observation → proposed tool → owner/device/run/epoch/action-ID checks → guest execution → post-action observation → persisted result → next round → independent completion audit.

Successful input injection is not completion evidence. Unknown outcomes require observation before retrying. Skill reads are not phone actions; previews are not complete messages; drafts are not sent messages. The UI preserves public progress, actual tool I/O and audits without inventing private chain-of-thought.

Task attempts progress independently on different devices. PostgreSQL grants exclusive device leases during admission; an attempt retains its primary and borrowed devices until its result is committed. Model and guest I/O hold no SQL transaction. Cancellation targets a run, while foreground device changes leave other runs active. Cross-device control waits for an occupied device instead of taking another task's ownership; device messages provide durable handoff. Startup releases the previous process's leases while preserving checkpoints, unknown actions and wait deadlines.

An occupied device returns `device_busy` before dispatching input, with a wait or handoff recommendation. It does not trigger screenshot recovery. Other errors retain their execution-status contract; unknown outcomes still require observation.

Blocked attempts and successful waits do not consume the execution-failure budget or count as execution progress. Actual failures retain the same threshold, including invalid wait and plan requests.

Responses, Chat Completions and native Anthropic Messages use the same engine. The latter translates tools, results and images; truncated, filtered, unterminated or malformed tool outputs are rejected. Providers must pass actual text, image and tool round-trip probes.

A Responses completion event with an explicit non-completed response status is rejected before its output can become tool proposals. Complete output is preserved verbatim for the existing evidence and provider-replay paths.

## Skill storage

PostgreSQL owns committed Skill text. Each installation serializes its file projection with a per-Skill operating-system lock, then atomically publishes `SKILL.md` and synchronizes it to disk. The blocking writer retains the lock until file operations finish, including after cancellation of its async caller. File I/O holds no database transaction; reads and startup reconcile the projection from durable text while preserving auxiliary resources.

## Devices and exchange

The default is one phone and one computer. Root/sudo apply to guests, not arbitrary host execution. A persistent device link gates file exchange, with device/direction/path/hash checks; disconnecting rejects new transfers. Conversations and drafts remain per device, even for a shared Agent.

A real test-file phone → computer → phone workflow has been verified; this is not real-account WeChat delivery verification. Durable action retention and isolated worker/database fault recovery passed. Actual host sleep/reboot, prolonged multi-device operation and notification/task contention still need release acceptance. See [task recovery](task-recovery.md) and [release status](release-status.md).

Managed staging separates versioned source and persistent user state. The development workspace keeps its existing data directory. See [distribution and updates](distribution.en.md).
