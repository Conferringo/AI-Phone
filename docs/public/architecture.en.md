# Technical architecture

[简体中文](architecture.zh-CN.md)

Next.js App Router/Turbopack, React, Tailwind, shadcn/ui and GSAP provide the workbench. Rust/Axum/Tokio own durable runs, model adapters, tools, permissions and completion auditing. PostgreSQL/SQLx store registry and conversation state; Redis is declared but is not the task queue.

Android runs natively through Android Emulator, controlled by a Rust runner, ADB and guest bridges. Native UI targets with OCR/screenshot fallback ground actions. Bun bridges use Android gRPC and FFmpeg/X11 for JPEG frames and PCM audio. Native touch indicators/pointers are captured in the same frame. The Linux computer uses Docker, Debian/Openbox and Xvfb, with screenshot-based control and read-only regional zoom.

Business APIs use protobuf (`packages/protocol/proto`). Browser requests use `application/x-protobuf`; streaming business events use protobuf WebSocket frames. Media and health checks are separate. Model SSE is a separate transport.

## Execution

User/paired channel → durable run → pinned Agent/model revision → Skills/current observation → proposed tool → owner/device/run/epoch/action-ID checks → guest execution → post-action observation → persisted result → next round → independent completion audit.

Successful input injection is not completion evidence. Unknown outcomes require observation before retrying. Skill reads are not phone actions; previews are not complete messages; drafts are not sent messages. The UI preserves public progress, actual tool I/O and audits without inventing private chain-of-thought. The current worker is serial; multiple configured Agents do not imply concurrent execution.

Responses and Chat Completions use the same engine. The latter translates tools, results and images; truncated, filtered, unterminated or malformed tool outputs are rejected. Providers must pass actual text, image and tool round-trip probes.

## Devices and exchange

The default is one phone and one computer. Root/sudo apply to guests, not arbitrary host execution. A persistent device link gates file exchange, with device/direction/path/hash checks; disconnecting rejects new transfers. Conversations and drafts remain per device, even for a shared Agent.

A real test-file phone → computer → phone workflow has been verified; this is not real-account WeChat delivery verification. Durable action retention and isolated worker/database fault recovery passed. Actual host sleep/reboot, prolonged multi-device operation and notification/task contention still need release acceptance. See [task recovery](task-recovery.md) and [release status](release-status.md).

Managed staging separates versioned source and persistent user state. The development workspace keeps its existing data directory. See [distribution and updates](distribution.en.md).
