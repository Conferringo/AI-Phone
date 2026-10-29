# Protocol boundaries

Self-owned browser business requests use `application/x-protobuf`; streaming run events use protobuf WebSocket frames. New location configuration, state, guest commands and receipts share `packages/protocol/proto/ai_phone/v1/location.proto`, generated for Rust and TypeScript. PostgreSQL stores location state as binary protobuf. Media bytes and health responses retain their documented exceptions.

JSON remains where a different boundary requires it: external model providers, Chromium CDP, network-location providers, Tauri's command envelope, dependency manifests and configuration files. JSONB conversation/model context is persisted domain data, not the browser API. The older desktop process bridge and device-network stdin bridge still have JSON envelopes and require separate compatibility migration; this increment does not claim that all JSON in the repository has disappeared. New location guest command/receipt envelopes are already protobuf.

Bun is the sole JavaScript package manager for this repository. `scripts/bootstrap_bun.py` downloads the pinned executable with a reviewed SHA-256, without npm. `scripts/bun` is the project entry. Node.js remains a runtime dependency for tools that require it; no npm install/run command is needed in owned setup or CI.
