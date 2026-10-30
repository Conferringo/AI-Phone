# Release gaps and capability roadmap

Updated 2026-10-30. Source and technical documentation are public. Complete four-platform distributions remain in development; a connection shell or an online source bootstrap does not meet the product contract.

## Remaining gates

Database runtime privileges, backup/restore and action-ledger retention have evidence. Remaining gates are owned by `config/release-policy.json`, with current conclusions in [release status](release-status.md). This is not a completion percentage.

- **UI/platform acceptance:** first-run configuration and selected window flows have been exercised. The full window/language/input matrix, native audio, mobile keyboard safe areas and visual acceptance remain.
- **Clean-host installation:** CLI and isolated installation tests exist. The default Android phone is still prepared by deployment tooling; the web Android provisioner targets the additional-instance path. A new configuration guide is not an end-to-end installer. Clean macOS hardware, interrupted installs and Windows/Linux host support need evidence.
- **Licenses/assets:** original code uses Apache-2.0. The locked native Rust/Android notice-text gaps have been addressed. Full Bun/browser/emulator/guest-image redistribution, corresponding-source and asset review remain. Source scanning does not clear copyright or redistribution requirements.
- **Complete device bundle:** ship clean Android/Linux baselines, matched emulator and prebuilt services; first setup must not ask users to acquire images or compile a backend. The prebuilt [web component](runtime-bundles.md) is one implemented part.
- **Long-running recovery:** worker fault injection and restoration have evidence. Host sleep/wake, reboot, extended background operation and complete upgrade rollback remain.

Native CI has compiled macOS ARM64, Windows x64, Linux x64/ARM64 and Android ARM64 targets; current revision results remain separate. Android independent hosting targets root/KVM devices, not ordinary unrooted phones. Non-macOS physical installation tests are waived by the maintainer and remain unverified; complete contents, compilation, audits and redistribution checks are still required.

## Delivery order

1. Complete a clean macOS ARM64 host path: dependencies, resources, phone/desktop creation, model configuration, first task and file exchange, including interruption recovery.
2. Prepare the audited source preview with bilingual docs, architecture, privacy/disclaimer, locked dependencies, notices and SHA256 manifests. The reviewed, state-free source is public on master; no user state or private development history is uploaded.
3. Validate and sign the client candidates, with independent application and user-state directories.
4. Implement Windows/Linux RuntimeProvider backends and accept each clean-host path before advertising support.
5. Complete updates: verify versioned artifacts, back up state, drain new actions, migrate, check health, activate or roll back. A direct git pull over the working/user directory is not an upgrade strategy.

## Models

Keep model/API settings, external channels and memory within each Agent. Device bindings remain explicit; skills, knowledge and working files are shared. Live validation currently covers the configured DeepSeek service. Protocol compatibility does not guarantee vision or tool competence.

Next: model discovery with manual-ID fallback, separate text/vision/tool/context checks tied to configuration revision, then provider-specific contract tests and live regressions. Native Anthropic Messages is implemented alongside Responses and Chat Completions, with 13 presets and isolated protocol tests. New-provider live API qualification, OAuth and additional local vision integrations still need evidence.

## Voice and calling — planned, not implemented

- **V1:** per-Agent STT/TTS adapters, independent microphone/device/playback tracks, local and cloud providers, transcription fallback and clear recording state.
- **V2:** endpoint detection, echo handling, interruptible playback, ordered transcription receipts and playback generations. Reuse the durable conversation/steering path; interruptions must not replay device side effects.
- **V3:** a mock/SIP CallSession and TelephonyProvider. Persist intent before dialing, deduplicate callbacks, query uncertain results, bound DTMF and duration, and reject late audio after hangup.
- **V4:** user-configured telephone services or physical devices, explicit recipient/cost/recording scope and verified call-state evidence. Regional telecom/recording requirements need review before release.

An emulator dialer, root access or a device identity does not provide a carrier line, phone number or working eSIM. Public provider webhooks conflict with the current LAN-only/no-tunnel deployment boundary; they must not be silently enabled. An explicitly configured internal SIP service is a separate deployment option.

Suggested responsibilities: DeviceRuntime owns devices, AgentRuntime execution and continuous input, MediaSession audio, TelephonyProvider the line. PostgreSQL owns durable sessions and the action ledger; Redis remains reserved for optional caching. New business APIs start with protobuf, with separate negotiated media transport. Voice must preserve the existing run/device/epoch/action identity boundaries.

Apache-2.0 applies to original code only. Third-party assets retain their licenses. Disclaimers describe actual scope and data flows; they cannot substitute for authorization or abuse controls. See the [source comparison](comparative-research-2026-10-29.md) and [UI audit](release-ux-audit-2026-10-29.md).
