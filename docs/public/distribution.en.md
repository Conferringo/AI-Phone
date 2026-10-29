# Clients, servers, user data and updates

[简体中文](distribution.zh-CN.md) · Source packaging and verified staging exist; a macOS native-client preview exists; full host installers and automatic updates remain open.

| Platform | Client target | Server target | Current status |
| --- | --- | --- | --- |
| macOS | Signed/notarized DMG | Native ARM64 Android + Docker services/desktop | Source preview and native .app/DMG for an existing deployment; no Developer ID signing/notarization |
| Windows | Signed MSI/NSIS | Native Android + WSL2/Docker adapter | Paths, ADB ownership, OCR and virtualization need implementation |
| Linux | AppImage/deb/rpm | KVM Android + Docker + systemd | Next server target; guest discovery, OCR and unattended operation need work |
| Android | APK connecting to the user's server | Full server on an ordinary unrooted phone is not promised | No APK built; on-phone hosting is a separate experimental target |

A shared lightweight client shell should offer Local deployment or Connect to server and reuse the Web UI. Remote pages must not acquire arbitrary native execution privileges. [Tauri supports distribution workflows across these platforms](https://v2.tauri.app/distribute/); a working macOS ARM64 preview now exists in apps/client; other clients and full host installation are not accepted. TLS/trusted-certificate onboarding and client/server protocol compatibility are release requirements. Every user owns their server/model/device/proxy/channel settings; there is no default connection to the developer's host.

## Storage layout

Managed staging creates `app/releases/<version>/`, `current`, and persistent `user/.env` plus `user/data/`. A release's data and .env are links into user storage. Initial staging creates current; subsequent releases are staged without live activation. The existing development directory is not migrated automatically.

PostgreSQL still uses a Docker named volume: copying user alone is **not a complete backup**. Existing backup/restore covers the database, Android state, Linux rootfs/home and private files; isolated restore and restored guest boots passed. Automatic update/activation remains open. Built-in Skills belong to the application version; private knowledge, memory, learned experience, user Skills and disabled-Skill preferences must survive updates. System/resource changes are separate operations, never incidental effects of a UI update.

## Update lifecycle

Do not run git pull inside the active installation. The intended update action checks a signed release manifest, presents changes/compatibility/disk requirements, waits for task checkpoints, backs up data, verifies the downloaded signature/hashes, stages a version, runs explicit migrations and health checks, switches current atomically and keeps recovery points.

Application rollback and database restore are different. An irreversible migration requires restoring compatible data, not just selecting an old binary. Unknown device actions or message deliveries must not be replayed blindly after restart.

Current stage supports local archive checks, bounded extraction and version staging. It does **not** provide publisher authentication, remote version discovery/download, migrations, live activation or one-click restore. An update button will be added only when connected to that working lifecycle. Git remains a developer workflow; public release packages may later use GitHub Releases, never upload user data. No public repository/release has been published in this work.

Delivery sequence: clean-host macOS source preview and backup/restore; Linux server support and full network recovery coverage; unified four-platform signed clients and real installation/update tests. Compiling four client shells would not establish four server platforms.

Recovery remains quarantined, with an offline `recover-desktop` validation command. See [backup and recovery](backup-restore.en.md) and the [release contract](release-contract.en.md).
