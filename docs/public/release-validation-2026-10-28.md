# Installation, permissions and recovery validation

2026-10-28 · internal source candidate. No public upload.

| Area | Actual evidence | Limit |
| --- | --- | --- |
| Database permissions | Production assistant connects as `ai_phone_runtime`; explicit migrations as `ai_phone_migrator`; bootstrap administrator unchanged. Three dedicated tests cover fresh/legacy/idempotent/refused migration and business CRUD; seven types of escalation/DDL/history/password-table operations denied. | Single-owner application, not tenant RLS. Cluster administration remains trusted. |
| Full backup | Consistent quiesced backup, 12.44 GB, 3,019 state files; original services resumed. PostgreSQL, active/archived phone disks, Linux rootfs/home, configuration, keys, files and durable action records included. | Private, not encrypted by the tool; never in the source bundle. |
| Actual restore | Empty isolated PG/directory, 33 table fingerprints matched. 300 messages, one account and original paused run retained. Android disk booted, root/screenshot/WeChat/Clash verified offline; exported Linux booted GUI/root/browser/office/Clash/TUN and 10 work files matched. | Same host. Test copies cleaned; originals retained. Restore stays quarantined; automatic failover/activation is not implemented. |
| Failure behavior | Six archive/restore regressions: sparse files, links, user log/lock retention, malformed paths, corrupt payload, occupied destinations, directory envelopes, idempotent relocation and full transaction rollback on role refusal. Direct assistant binary refuses quarantine before binding or database/device access. | Host power loss and broken backup media have not been simulated. |
| Independent source preparation | Actual source envelope stages into app/user layout; its own node_modules/target compile protobuf, TypeScript, Rust and production Web; empty PG initializes 24 SQLx migrations and 7 built-in Skills, zero accounts/messages. Reproducible script: `scripts/verify_source_install.py`. | Uses this host's installed SDK/JDK/Docker and package caches. It does not start guests or set up accounts. Default 16 GB fresh-phone boot needs more free disk and was not marked passed. |
| Regressions | 54 regular assistant tests (9 ignored remain uncounted), 25 runner tests, 7 packaging/staging tests, Buf/TypeScript/Cargo checks, production Web, runtime-role check and live protobuf smoke. | Provider/channel real-account and full UI/language/accessibility matrices remain separate. |
| Dependency metadata | 287 Rust and 319 JavaScript package versions/licence metadata plus notice-file hashes, zero missing metadata. | Metadata is not legal clearance; original code is now Apache-2.0; final artwork acceptance, pet and binary/source-offer review remain open. |

Public packaging remains blocked by clean physical-host installation, complete UI acceptance, licences/provenance and long-running host failure recovery. The backup/restore, database runtime role and durable action-retention gates have evidence. Four-platform signed clients and the automatic updater are subsequent product work.

See [roles](database-roles.md), [backup/recovery](backup-restore.en.md), [Chinese recovery guide](backup-restore.zh-CN.md), [dependency inventory](dependency-licenses.json), and [release status](release-status.md). Private logs/evidence and all user data are excluded from source packages.
