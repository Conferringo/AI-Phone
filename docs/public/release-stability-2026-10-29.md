# Stability and audio audit / 稳定性与声音复查

2026-10-29 · internal source preview. This report records tested behavior, not a stable-release certification.

## 修复 / Changes

- Linux audio: supervise PulseAudio independently of the desktop session. Recreate the `aiphone` sink/monitor after a daemon crash. The desktop health check now includes audio availability.
- Guest media ownership: each capture has one guest process owner. Connection EOF or a five-second lost heartbeat reaps its ffmpeg process group. Killing only the host `docker exec` client previously left captures running.
- Device playback: one audio graph per device, independent of focus and network reconnection. A reconnect clears stale samples without suspending the graph; a suspended context cannot be reported as successfully playing. S16LE stereo decoding preserves partial frames and caps queued live audio.
- Quiet Android reconnect: one silent stereo frame flushes buffered HTTP proxies at stream startup. This is transport pre-roll, never evidence of guest-generated sound. A quiet Android source may legitimately wait indefinitely for its next packet.
- Skill persistence: SQL is the committed source of truth; resource files are recoverable projections. Imports stage resources outside the visible Skill directory. An uncertain COMMIT re-reads durable state instead of restoring stale text.
- Shared PostgreSQL restore: existing global roles are not rotated or re-granted. A backup with incompatible role credentials is refused before restore. PostgreSQL-native SCRAM/MD5 verification tests cover matching, mismatched and malformed records without logging passwords/verifiers.
- Rust boundaries: HTTP rejections use a small owned wrapper; run checkpoints and mutable steering inputs are named structures. Strict Clippy passes without suppression. This does not resolve all large-module architecture debt.

## 验证 / Verification

- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `cargo test --workspace --locked`: 81 ordinary tests passed; integration tests require explicit isolated fixtures.
- Two complete isolated database/recovery passes in this batch. Final pass: 21 Rust integration tests, 4 role/credential tests, 8 backup/restore tests, and 5 process-fault scenarios. These include lost COMMIT acknowledgement, SIGKILL, PostgreSQL restart/outage, suspended workers, 429/503 recovery, incomplete model streams, unknown-result refusal and 56 persisted tool turns.
- Guest audio suite ran twice: audio-daemon crash recovery, repeated disconnect/host-client kill, expired heartbeat. No orphan ffmpeg remained in the fixture.
- Rebuilt desktop image booted GUI plus audio in an isolated container with no existing home volume and no network.
- Real Android native audio playback and real Linux paplay both produced non-zero captured PCM. Three concurrent authenticated listeners per device received data; revoking only the test session closed its media within 4.7 seconds.
- Native macOS client: both audio buttons, volume dragging, restart and transport reconnect were exercised. A quiet reconnect no longer leaves the preceding disconnect error displayed. Physical speaker audibility still requires listener confirmation; captured PCM alone does not prove the final speaker output.
- Playback module: measured coverage gives all 12 functions CRAP 1–6. Mutation testing killed 47/50 changes. The three survivors are equivalent for this module: equal stereo-channel lengths (two sites) and a time fallback when there is no gain node. Raw score remains 94%, not a claimed whole-project 100%. Duplicate scan at 0.8 found no candidates in this module.

Test state is isolated from existing accounts, conversations, guest disks and user files. Existing messages were compared before and after assistant deployment; no conversation clearing or device recreation was used. Private logs, tone fixtures and application screenshots are excluded from the source export.

## 仍需完成 / Open release work

A public stable release remains blocked by the clean physical-host installation test, full native platform/media/input acceptance, asset/dependency redistribution review, and actual host sleep/reboot/power-loss plus sustained multi-device operation. The prepared clients still require a server deployment; they are not four complete server installers. Signing/notarization and production update distribution are separate work.

Native automated text entry into the Android browser exposed a punctuation/paste ordering issue during the audio check. The test returned to the prior page and used the native player instead; keyboard/IME acceptance is not claimed complete.

See [release status](release-status.md), [SOLID audit](solid-audit.md) and [multi-platform client evidence](multiplatform-clients.md). No public repository or release upload was made in this batch.
