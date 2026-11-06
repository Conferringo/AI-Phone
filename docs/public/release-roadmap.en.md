# Roadmap

AI Phone is developing complete local distributions for macOS Apple Silicon, Windows x64 and Linux. Current downloads and platform limits are listed in [release status](release-status.md). Planned capabilities below are not included until their corresponding implementation and qualification are complete.

## Complete local installation

- Include clean Android and Linux baselines, matching emulators, prebuilt services and their execution environments in each distribution.
- Create the phone, computer and model configuration from one client, with resumable resource preparation and device readiness checks.
- Keep device resource presets configurable while preserving existing disks, accounts and conversations. The default workspace contains one phone and one computer.
- Support interruption recovery, host restart and a complete update path. Physical installation on all desktop platforms remains untested under the current preview waiver; compilation, complete package contents and redistribution review remain required.

The installation contract is described in [distribution requirements](preview-release.md), [runtime bundles](runtime-bundles.md) and [self-contained hosts](integrated-hosts.md).

## Responsiveness and sustained work

Reduce cold-start time, control latency and input-to-response delay. Measure cold and warm operation separately across device capture, transport, model scheduling and playback. New user input should adjust the current task without waiting for a long-running foreground operation or recreating its conversation.

Device actions retain their result identity across cancellation and recovery. Unknown outcomes are observed before retrying. Long-lived background tasks keep their checkpoints while notifications and user instructions are admitted through the same durable conversation path.

## Models and connections

Keep model/API settings, application connections and memory within each Agent. Device bindings are explicit; skills, knowledge and work files are shared.

Extend model discovery with manual-ID fallback, configuration-bound capability checks, provider-specific protocol tests, OAuth and local-model integration. Protocol compatibility does not establish every model's visual or tool competence. Supported protocols and providers are listed in [model setup](model-and-channel-setup.md) and [compatibility](compatibility.md).

## Copilot voice

The [Copilot contract](copilot-voice.md) defines continuous listening, 1.5-second commits, transcript corrections, same-conversation steering and interruptible spoken feedback. Browser PCM capture, per-Agent speech adapters, durable text/correction submission and streamed, interruptible WAV playback are implemented and covered by isolated transport tests. These checks do not establish real microphone or speech-provider quality.

1. Qualify real microphones and configured speech providers across languages, corrections, noise, echo and device switching.
2. Measure capture-to-final-text, first spoken reply, playback interruption and first real device action separately.
3. Extend provider protocols and native speech-to-speech without coupling microphone, device-audio and playback ownership.
4. Provide an explicit guest-microphone route for voice messages and designated app-call tests.

## Calls and extensions

A call capability needs its own session, line provider and verified dial/answer/mute/hangup lifecycle. SIP or user-configured telephone services and physical devices are separate extensions. An emulator, root access, device name or eSIM screen does not provide a carrier line or telephone number. Recipient, cost and recording scope must be explicit before real-call delivery.

Optional [companion applications](companion-extension.md), additional devices and system templates can extend the core workspace independently. Existing user state must remain compatible across extensions and program updates.

## Updates and distribution

Use signed versioned artifacts, compatibility/capacity checks, consistent backup, explicit migration, health checks and activation/rollback. A direct Git pull over user state is a contributor operation, not an update strategy. See [application and user-data boundaries](distribution.en.md) and [backup/recovery](backup-restore.en.md).

Original code remains Apache-2.0. Third-party software, images and assets retain their own licences and source obligations. Distribution review and physical installation scope are recorded separately from compilation and protocol support.
