# Roadmap

See [release status](release-status.md) for downloads and platform requirements.

## Complete local installation

- Include clean Android and Linux baselines, matching emulators, prebuilt services and their execution environments in each distribution.
- Create the phone, computer and model configuration from one client, with resumable resource preparation and device readiness checks.
- Keep device resource presets configurable while preserving existing disks, accounts and conversations. The default workspace contains one phone and one computer.
- Build the offline Windows Linux host around QEMU and Windows Hypervisor Platform. Validate boot, Android pairing, desktop control and restart on a clean Windows 11 x64 host.
- Qualify installation recovery, host restart and updates on physical macOS and Windows hosts.

The installation contract is described in [distribution requirements](distribution-requirements.md), [runtime bundles](runtime-bundles.md) and [self-contained hosts](integrated-hosts.md).

## Responsiveness and sustained work

Reduce cold-start time, control latency and input-to-response delay. Measure cold and warm operation separately across device capture, transport, model scheduling and playback. New user input should adjust the current task without waiting for a long-running foreground operation or recreating its conversation.

Device actions retain their result identity across cancellation and recovery. Unknown outcomes are observed before retrying. Long-lived background tasks keep their checkpoints while notifications and user instructions are admitted through the same durable conversation path.

## Models and connections

Keep model/API settings, application connections and memory within each Agent. Device bindings are explicit; skills, knowledge and work files are shared.

Extend model discovery with manual-ID fallback, configuration-bound capability checks, provider-specific protocol tests, OAuth and local-model integration. Supported protocols and providers are listed in [model setup](model-and-channel-setup.md) and [compatibility](compatibility.md).

## Copilot voice

The [Copilot contract](copilot-voice.md) defines continuous listening, 1.5-second commits, transcript corrections, same-conversation steering and interruptible spoken feedback. Browser PCM capture, per-Agent speech adapters, durable text/correction submission and streamed, interruptible WAV playback are implemented and covered by isolated transport tests. Real microphone and speech-provider validation are pending.

1. Qualify real microphones and configured speech providers across languages, corrections, noise, echo and device switching.
2. Measure capture-to-final-text, first spoken reply, playback interruption and first real device action separately.
3. Extend provider protocols and native speech-to-speech without coupling microphone, device-audio and playback ownership.
4. Provide an explicit guest-microphone route for voice messages and designated app-call tests.

## Calls and extensions

Calls need a session, line provider and dial, answer, mute and hang-up controls. Extensions can integrate SIP, user-configured telephone services or physical devices. Show the recipient, cost and recording scope before placing a call.

Optional [companion applications](companion-extension.md), additional devices and system templates can extend the core workspace independently. Existing user state must remain compatible across extensions and program updates.

## Updates and distribution

Use signed versioned artifacts, compatibility/capacity checks, consistent backup, explicit migration, health checks and activation/rollback. See [application and user-data boundaries](distribution.en.md) and [backup/recovery](backup-restore.en.md).

Original code remains Apache-2.0. Third-party software, images and assets retain their own licences and source obligations.
