# Source technical preview

2026-10-29 update: the maintainer authorized an experimental public source preview and waived separate-device installation as a source-publication prerequisite. This is not a passed installation check. Only a fresh allowlisted source/documentation tree is uploaded; no local history, state, installers or private evidence. Product acceptance and binary notices remain separate.

[简体中文](source-release.zh-CN.md) · 0.1.0-preview.1 · 2026-10-28

This batch publishes an experimental source preview and a locally reviewable source tree. Original code is [Apache-2.0](../../LICENSE); third-party packages, reference assets and guest applications retain their respective terms. See [third-party notices](../../THIRD_PARTY_NOTICES.md).

The package includes the Next.js workbench, Rust execution engine, Android bridges, Linux desktop recipe, protobuf contracts, built-in Skills, installation/backup/recovery tools, tests and GitHub workflow definitions. Default: one phone and one computer, each bound to an Agent. PostgreSQL persists runs; Redis is optional, not the active task queue.

Developer Git history, keys, subscriptions, accounts, chats, disks, attachments, private Skills and real screenshots are excluded. Export uses an allowlist, never a copy of the entire development tree. Historical one-off destructive/verification scripts, the retired validation APK and old mock device-lab route are excluded. These are retired development artifacts; the working runtime/input/notification/knowledge components remain included.

## Start from source

The verified server platform is macOS Apple Silicon. Install Node 22+, Bun (project bootstrap), Rust 1.88+, Python 3.11+, Java 17, Android command-line tools, Xcode command-line tools and Docker Desktop. Reserve 35 GiB for a full first install and a default 16 GB phone.

```sh
./ai-phone doctor
./ai-phone install
```

The Android SDK prompts for licence acceptance. The host defaults to direct networking; each guest has its own Clash app and the user supplies subscriptions. Enter the workbench and configure model/API/device bindings under Configuration → Agent. No developer keys or accounts are shipped. Repeated startup restores only missing services.

This source preview has prerequisites. Independent source builds, empty-database initialization and same-host full-state recovery passed; installation on another clean physical host, initial Agent setup and the complete handoff remain unverified. See [setup](getting-started.en.md), [architecture](architecture.en.md) and [recovery](backup-restore.en.md).

## Prepare a local review repository

```sh
python3 scripts/check_source.py
python3 scripts/source_export.py --destination /private/ai-phone-review --git
```

The exporter refuses existing destinations. It produces a manifest and a fresh local Git repository with one commit and no remote. Public agent instructions come from a dedicated guide, not the private local AGENTS.md. The draft marker is not publication approval. `package --public` remains blocked by product acceptance gates.

The pinned `.github/workflows/source.yml` has read-only repository permissions. It checks protobuf/TypeScript/Rust, isolated UI/adapters, installation/archive tests, production Web, disposable PostgreSQL integration and worker process faults. Tests that restart PostgreSQL run sequentially. It never publishes, calls paid models or operates real devices. Linux CI compilation does not establish Linux server support. Equivalent commands are tested locally; consult the repository Actions for remote results, rather than inferring success from a prepared workflow.

Open work includes clean physical-host installation; host sleep/reboot/power-loss and prolonged multi-device recovery; complete language/accessibility and real-channel matrices; signed four-platform clients and automatic updates; existing-device resource migration; final binary/image redistribution notices/source obligations. The welcome/forest entry has been removed in favor of direct workbench startup. The original knowledge motion passes code/lifecycle tests; its full real-page visual acceptance remains open.

Current source-preview evidence is separate from a stable product claim. The sequence is macOS source preview, Linux server support, updater/unified client, then native-platform signing and installation acceptance. An ordinary Android phone is not promised to host the whole server.

## Published source and subsequent changes

Repository: [Conferringo/AI-Phone](https://github.com/Conferringo/AI-Phone). Subsequent work should be reviewed against a checkout of that repository: generate a new allowlisted export, compare it with the checkout, then commit the reviewed changes while preserving public history. Never add the entire live deployment directory, force-replace its history or run Git-based updates over user state. Release artifacts remain separate.
