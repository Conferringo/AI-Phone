# Clients, runtimes, user data and updates

[简体中文](distribution.zh-CN.md) · Updated 2026-10-30

The product is an application that deploys its own devices and services. Users configure resources and models in the client; an existing-server connection remains an optional development/remote path. Complete installers are not yet available. The source and documentation are [public](https://github.com/Conferringo/AI-Phone).

| Target | Build/runtime evidence | Remaining independent-host work |
|---|---|---|
| macOS ARM64 | Existing native Android/Linux operation, client setup and native bundles | Complete clean offline payload, first run and update activation |
| Windows x64 | Native client builds; each revision also checks relocated web resources | Device/backend supervision, paths, permissions, OCR and virtualization |
| Linux x64 | Native builds and managed KVM/systemd installation code | Complete payload and first-run acceptance |
| Linux ARM64 | Native client and prebuilt web builds | Reviewed matching Android runtime and complete host integration |
| Android ARM64 root/KVM | Client APK builds | Privileged host checks, matching runtime and both independent devices |

Non-macOS physical installation is waived by the maintainer; untested behavior remains unverified. This does not waive complete contents, compilation or redistribution checks. Signing/notarization and signed updates remain open. No client defaults to a developer's server, credentials or network subscription.

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

`scripts/client_host_entry.py` validates and stages this hash-addressed layout. Existing bytes, aliases and persistent links are checked on retry; occupied or changed code is rejected. It does not replace existing user state. The prebuilt [web component](runtime-bundles.md) contains Next standalone and pinned Bun; the remaining full-runtime components are still required.

The contributor CLI's `scripts/release_stage.py` is a separate, version-addressed layout: `app/releases/<version>`, an initial `current` pointer and persistent `user`. Later stages do not activate a running version. Its layout is not silently converted to native setup's layout. The existing development checkout also stays in place; installing a client does not migrate its guests automatically.

PostgreSQL uses a Docker named volume. **Copying `user` alone is not a complete backup.** Backup/recovery covers the database, Android state, desktop rootfs/home and private files. Recovery goes to an empty target and stays quarantined until explicit validation/activation. Built-in Skills belong to the code version; user Skills, learned experience, memory, knowledge, attachments and disabled-Skill preferences must survive updates. Device resizing and system replacement are separate operations from application updates.

## Update contract

The intended update sequence is signed version discovery → compatibility/capacity check → task checkpoint → consistent backup → signature/hash verification → staging → explicit migrations and health checks → activation or rollback. A failed backup prevents activation. Application rollback and database recovery are distinct; irreversible migrations need compatible restored data. Unknown device actions or message deliveries are observed before retrying.

Current code supplies local integrity checks, staging and recovery tools. It does not yet provide authenticated remote update discovery, automatic live activation or one-click upgrade rollback. No disconnected update button is offered. A direct `git pull` over user state is not an updater; Git remains a contributor workflow.

Full packages must contain clean Android/Linux baselines, emulator, prebuilt services and matched runtime components, with provenance, notices and applicable corresponding source. Device/account data is never part of these baselines. See [distribution requirements](preview-release.md), [release status](release-status.md), [backup/recovery](backup-restore.en.md) and the [original design baseline](release-contract.en.md).
