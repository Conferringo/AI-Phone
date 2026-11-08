# Private backup and recovery

[简体中文](backup-restore.zh-CN.md) · Apple Silicon macOS source installations.

The Rust `assistant --state-backup` command owns backup and restore.

Run `./ai-phone backup --output data/backups/my-backup.backup` from the installation. The tool verifies service ownership, quiesces this installation's services and guests, dumps PostgreSQL, exports the Linux root filesystem and archives private user state, then resumes the original services. Failure also attempts to resume them and reports restart failures.

The recommended `.backup` directory envelope avoids a second full-size copy during sealing. Optional `.tar` envelopes require extra temporary disk space. Keep the entire envelope and its SHA256 sidecar.

Backups contain accounts, messages, keys, attachments, knowledge, Skills, complete action records, preserved Android disks and Linux files. They are private, mode 0600, and must never enter public release archives. Use private encrypted storage.

Restore only into an empty directory and a separate empty PostgreSQL 17 database:

```sh
./ai-phone restore --archive /private/my-backup.backup \
  --destination /private/AIPhone-recovered \
  --pg-container my-empty-postgres --database ai_phone_recovered
```

The target container's local administrator must match the bootstrap login in the backup. A compatible Android SDK/system image must already be installed. Existing tables or files are refused. Schema/data/role grants are imported transactionally; every public table's row count and row hashes are checked. Private files are individually verified, sparse disks remain sparse, and Android backing paths are relocated against verified SDK files.

The recovered instance stays offline and quarantined. Pending tasks are paused; exploration and notification triggers are disabled. `data/restore-report.json` records verification. Linux rootfs and metadata remain in `data/desktop/recovery` for controlled import. Do not run cloned signed-in social accounts concurrently. Normal start refuses `data/restore-quarantine.json`.

Before activation, verify the source links, Linux import, device bindings and network setup. Stop the original instance.

`./ai-phone recover-desktop --destination /private/AIPhone-recovered --desktop-container aiphone-recovery-check` imports and starts an offline Linux verification container with no host ports and only restored mounts. It never replaces the original desktop.

An isolated recovery exercise verified database rows and state files, booted the restored Android disk with its installed apps, and opened the Linux desktop with restored work files.
