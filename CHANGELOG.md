# Changelog

## 0.2.2 Fabric

- Deliver files produced on Android and Linux as downloads in the assistant reply and editable copies in Work Files.
- Reuse the worker's current context for device tool checks, avoiding repeated PostgreSQL reads of screenshot data.
- Reuse the receive buffer for phone and desktop screen streams and copy only the newest complete frame per read.
- Stream chat attachment downloads through the assistant and web client.

## 0.2.1 Eclipse Beta

- Use a bundled multilingual Whisper model for desktop Copilot and read replies with an installed system voice.
- Skip redundant Copilot draft requests during pauses. The recorded scheduling test reduced 22 requests to 18 and preserved all final audio.
- Reuse the existing model input when building a request. A 276 KB dual-image fixture used about 60% fewer allocated bytes during request assembly and produced identical request bodies.
- Reap guest network commands before reporting a disconnected control session as cancelled.
- Sign the macOS app and disk image ad hoc and the Windows installer with a temporary Authenticode certificate. Follow the [Beta installation guide](docs/public/release-status.md#install-the-beta).

## 0.2.0 Daemon

- Route a completion draft from a single Android Settings action directly to independent verification. Six paired runs cut median planning calls from 3 to 2 and median completion time from 6.18 to 5.02 seconds.
- Seal the complete macOS application bundle with an ad hoc signature.
- Sign the Windows installer with a temporary Authenticode test certificate.

## 0.1.9 Cock

- Use the recorded post-action frame to verify desktop input and bind execution receipts to the owning task.
- Shorten verified Android Settings navigation and wake Liberty as soon as it is enabled.
- Route cross-Agent handoffs to the receiving Agent. Complete file transfers using the verified receipt.

On the same real Android Settings task, across three runs per version, median time to the first device action went from 1,905 ms to 1,630 ms and independent verification went from 2,437 ms to 1,442 ms. Median task completion went from 6,584 ms to 6,981 ms as model decision times varied. One Liberty activation comparison went from 2,440 ms to 25 ms. A real cross-Agent file handoff took about 32 seconds in both versions, with matching file digests and a verified receipt.

## 0.1.8 Bibliography

- Introduce AImanity and rename autonomous exploration to Liberty.
- Hold undispatched Copilot actions while the user speaks.
- Retain chosen Liberty goals and reuse verified cross-application knowledge.
- Observe meaningful desktop changes and support authorized Harness collaboration.
- Publish the signed update feed separately from the two installers.

## 0.1.7 Relay

- Wake connected-app delivery immediately after task completion using local Redis.
- Start device handoffs as soon as PostgreSQL commits the request.
- Export accepted device handoff events to optional Kafka for replay.

## 0.1.6 Apocalypse

- Review the version, name, and notes from the workbench update icon. Confirm to download, verify, install, and restart.
- Continue interrupted tasks from their saved checkpoints.
- Expand model provider options and preserve native tool results across model turns.
- Reduce device observation work, Linux screen confirmation time, and background workbench requests.
- Store WhatsApp sessions in PostgreSQL and index the current task lookup.

## 0.1.5

- Record the first model text separately from the first network bytes.
- Start work on the selected computer without listing devices again.
- Refresh stale phone observations immediately when no action was executed.
- Wake on committed instructions and avoid redundant database reads during model work.

## 0.1.4

- Download, verify, install and restart from the macOS and Windows client.
- Reuse the existing device disks, model configuration and conversations.
- Read macOS host capacity without starting command-line tools.
- Reduce model configuration work when loading the registry.

## 0.1.3

- Start new chat requests promptly and refresh saved activity as it arrives.
- Wake the Agent on device notifications and keep progress when instructions are added to a running task.
- Reduce Android observation work by reusing screen dimensions and reading notification data in parallel.
- Handle DeepSeek thinking options on Anthropic-compatible endpoints.
- Require sign-in for access over the local network.
- Keep the existing Windows device disk during updates and check for client releases from the app.

## 0.1.2

- Locate the bundled QEMU manifest during Windows setup so the installer can enable the hypervisor and start the local devices.
- Ship the startup, screen, input and audio fixes on Windows as well as macOS.

## 0.1.1

- Restore the macOS workbench after local services finish starting.
- Read CPU and memory settings from existing Android devices with spaced configuration entries.
- Keep phone and desktop frames current when switching devices or reconnecting.
- Deliver confirmed IME text once and preserve the order of desktop clicks and keyboard input.
- Keep each device’s audio active when switching views and resume playback after an interruption.

## 0.1.0

- macOS Apple Silicon and Windows 11 x64 clients with bundled Android and Linux devices.
- Phone and computer control, independent device conversations, notification wake and autonomous exploration.
- Shared Skills, linked knowledge and work files. Model, memory and channel configuration per Agent.
- Local installation and recovery, protobuf business API and PostgreSQL persistence.
- Ten interface languages, including Arabic with right-to-left layout.

[Downloads and release notes](https://github.com/MrMilkDragon/AI-Phone/releases)
