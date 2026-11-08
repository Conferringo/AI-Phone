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

## Update contract

The intended update sequence is signed version discovery → compatibility/capacity check → task checkpoint → consistent backup → signature/hash verification → staging → explicit migrations and health checks → activation or rollback. A failed backup prevents activation. Application rollback and database recovery are distinct; irreversible migrations need compatible restored data.

Local staging and recovery are available. Signed update discovery and automatic rollback are planned.

Full packages must contain clean Android/Linux baselines, emulator, prebuilt services and matched runtime components, with provenance, notices and applicable corresponding source. See [distribution requirements](distribution-requirements.md), [release status](release-status.md), and [backup/recovery](backup-restore.en.md).
