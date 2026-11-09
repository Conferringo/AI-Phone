# Clients, runtimes, user data and updates

[简体中文](distribution.zh-CN.md) · Updated 2026-10-30

The installer supplies the devices and services. Configure resources and models in the client. See [downloads and host requirements](release-status.md).

## Implemented storage boundaries

Native setup uses Tauri's application data directory, with an owned `host` subdirectory:

```text
host/
  deployment.json             setup checkpoint
  tools/                      owned development bootstrap tools
  releases/<payload-sha256>/  verified versioned code and runtime component
    data -> host/user/data
    .env -> host/user/.env
  user/
    .env                      private database environment
    data/                     guests, account/model/channel state, files
```

`crates/host-runtime/src/source_cli.rs` validates and stages this hash-addressed layout. Existing bytes, aliases and persistent links are checked on retry; occupied or changed code is rejected. It does not replace existing user state. The prebuilt [web component](runtime-bundles.md) contains Next standalone and pinned Bun.

The contributor staging command uses the same native, content-addressed source stager.

PostgreSQL uses a Docker named volume. Backup/recovery covers the database, Android state, desktop rootfs/home and private files. Recovery goes to an empty target and stays quarantined until explicit validation/activation. Built-in Skills belong to the code version; user Skills, learned experience, memory, knowledge, attachments and disabled-Skill preferences must survive updates. Device resizing and system replacement are separate operations from application updates.

## Native client updates

The client downloads the stable release for its platform and verifies its Ed25519 manifest and installer digest. macOS replaces the application bundle and reopens it. Windows runs the existing Inno installer and relaunches the installed client. The persistent host directory stays outside the application bundle and installer payload. Windows updates reuse the pinned QEMU base and existing writable disk.

After verifying both installers, sign the update manifest on the release maintainer's machine:

```sh
./scripts/bun scripts/sign-client-update.ts /path/to/offline-key AI-Phone-0.1.7-macos-arm64.dmg AI-Phone-0.1.7-windows-x64-setup.exe AI-Phone-0.1.7-update.json
```

Attach the two installers and the signed manifest to the same stable GitHub release. Upload the same manifest as `AI-Phone-update.json` for workbench discovery and under its versioned filename for earlier clients. Keep the private signing key in the release maintainer's offline storage.

Full packages must contain clean Android/Linux baselines, emulator, prebuilt services and matched runtime components, with provenance, notices and applicable corresponding source. See [distribution requirements](distribution-requirements.md), [release status](release-status.md), and [backup/recovery](backup-restore.en.md).
