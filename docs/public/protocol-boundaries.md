# Protocol boundaries

Self-owned browser business requests use `application/x-protobuf`; streaming run events use protobuf WebSocket frames. New location configuration, state, guest commands and receipts share `packages/protocol/proto/ai_phone/v1/location.proto`, generated for Rust and TypeScript. PostgreSQL stores location state as binary protobuf. Media bytes and health responses retain their documented exceptions.

Live Copilot input uses binary protobuf WebSocket messages from `voice.proto` at `/api/voice/stream`. The initial frame pins device, Agent, voice configuration revision and capture session; subsequent frames carry bounded PCM or a turn commit. Incremental/final transcript events return over that connection. Next.js forwards the upgrade to Rust. Provider JSON/base64 is confined to the outbound realtime adapter; accepted text still enters the existing protobuf chat/Outbox contract. Audio frames stay in the live session.

Conversation history uses an explicit protobuf message window. The browser requests 40 messages per page; the server accepts at most 80 page rows, four run heads, eight watched messages and 32 exact input receipts. The browser merges each page into the device transcript by message ID. Older pages use a conversation-scoped exclusive anchor. Visible older ranges can be revalidated. Legacy full responses remain for existing clients and send/cancel/resume receipts until those consumers have compatible bounded representations.

External model adapters, Chromium CDP, network location, Tauri commands, manifests and configuration use JSON. PostgreSQL stores conversation and model context as JSONB. The desktop process bridge and device-network stdin bridge still use JSON; guest location commands and receipts use protobuf.

Bun is the sole JavaScript package manager for this repository. `scripts/bootstrap.sh` downloads and verifies the pinned executable. `scripts/bun` is the project entry. TypeScript build tools use Bun; first-party runtime services use Rust.
