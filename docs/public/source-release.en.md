# Source builds and contribution

[简体中文](source-release.zh-CN.md) · [Repository](https://github.com/Conferringo/AI-Phone)

The source contains the Next.js workbench, Rust execution services, Android bridges, Linux desktop recipe, protobuf contracts, built-in Skills, installation/backup/recovery tools, tests and CI workflows. The default workspace has one phone and one computer, each bound to an Agent. PostgreSQL persists task checkpoints; Redis is optional caching.

Original code uses [Apache-2.0](../../LICENSE). Dependencies, assets and guest applications retain their own terms; see [third-party notices](../../THIRD_PARTY_NOTICES.md). Source builds are intended for contributors. Complete end-user installers have separate contents and acceptance requirements in [release status](release-status.md).

## Prerequisites and startup

The locally exercised source host is macOS Apple Silicon: Node 22+, project-pinned Bun, Rust 1.96.0, Python 3.11+, Java 17, Android command-line tools, Xcode command-line tools and Docker Desktop. Reserve at least 35 GiB for a new full deployment; the default phone storage is 16 GB.

```sh
./ai-phone doctor
./ai-phone install
```

The Android SDK presents the terms for the requested packages. The host defaults to direct networking; each guest has a Clash app for user-supplied subscriptions. Configure models and device bindings in Configuration → Agent. No developer credentials or accounts are supplied. Repeated startup restores missing services while retaining existing state.

See [setup](getting-started.en.md), [runtime components](runtime-bundles.md), [architecture](architecture.en.md) and [backup/recovery](backup-restore.en.md). Non-macOS physical installer tests are outside the current acceptance scope and remain unverified; matching compilation and component checks are still required.

## Source and user data

Versioned code is separate from device disks, accounts, conversations, attachments and keys. Source export uses an allowlist with privacy and licence checks. Public contributor instructions come from a dedicated guide. Local environment files, private Skills, screenshots and evidence must stay outside public commits.

```sh
python3 scripts/check_source.py
python3 scripts/source_export.py --destination /path/to/new-review-directory --git
```

Export requires a new destination and produces a source manifest and local review repository without a remote. Review the exported diff against the public checkout before committing. Ongoing contributions preserve public history. Do not update a running user's state with a direct Git checkout; use versioned artifacts, backup, migration and recovery.

## Automated checks

Source CI checks protobuf, TypeScript, Rust, isolated UI/adapters, archive/installation contracts and a production web build. Prebuilt component verification stages and executes actual packaged files. Disposable PostgreSQL fixtures exercise migrations, role restrictions, backup/restore and worker recovery; restart tests run sequentially. CI does not use real devices, account credentials or paid model calls.

Native CI builds macOS ARM64, Windows x64, Linux x64/ARM64 and Android ARM64 clients on matching workers. A passing client build establishes compilation and inspected contents, not complete two-device hosting. Exact results and engineering scope belong in [review evidence](engineering/index.md); user-facing capability limits belong in [compatibility](compatibility.md).
