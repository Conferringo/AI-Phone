# Private installation state

The Rust host runtime owns profile validation, publication, source staging, installation progress and process ownership. Persistent user data is separate from versioned application resources.

| Owner | Responsibility |
|---|---|
| `crates/host-runtime/src/profile.rs` | Validate and publish device/resource profiles; reuse existing choices on resume |
| `crates/host-runtime/src/filesystem.rs` | Private atomic publication, directory synchronization and installation leases |
| `crates/host-runtime/src/source_cli.rs` | Content-addressed source staging and persistent user-directory links |
| `crates/host-runtime/src/unix_initializer` | Connect owned resources, engine, services and device readiness |
| `crates/host-runtime/src/wsl` | Windows backend ownership and initialization |
| `crates/assistant/src/database/bootstrap.rs` | PostgreSQL role preparation through SQLx |
| `crates/assistant/src/backup` | Private backup, transactional database restoration and device-state relocation |

Existing profile and credential formats remain readable. Reopening an installation does not replace its credentials or reconfigure its devices. Explicit changes use the same native validator as initial setup.

Source and runtime components are admitted before activation. The normal service login has DML permissions; schema migration uses its separate role. Restored state remains quarantined until activation, with pending tasks paused and automatic triggers disabled. Original installations and backups are retained.

On Unix, private files and directories use the existing permission contract and atomic publication. Windows ownership and filesystem operations use the native platform adapter. These controls belong to installation and persistence boundaries; phone actions and media frames do not repeat installation checks.

Current-source compilation, backup/recovery acceptance and full client lifecycle qualification are pending. See [release status](../release-status.md).
