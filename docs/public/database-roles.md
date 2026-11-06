# Database roles / 数据库权限

AI Phone uses three separate PostgreSQL logins:

| Login | Purpose | Rights |
| --- | --- | --- |
| Bootstrap account from private `.env` | Explicit role preparation, backup and restore | Database administrator; never used by the running Agent service |
| `ai_phone_migrator` | Explicit SQLx migrations | Owns application tables; schema/database CREATE, no superuser or role-management rights |
| `ai_phone_runtime` | Assistant service | SELECT/INSERT/UPDATE/DELETE on business tables, sequence access; schema history SELECT only |

Passwords for the two application logins are generated once in `data/database/credentials.json` with mode 0600. Keep this file and `.env` in private backups. They never enter the browser, protobuf response, source bundle or command-line arguments. The bootstrap account is retained for recovery; the migration does not rename, demote or replace it.

macOS, Linux and WSL startup call `ai-phone-assistant --prepare-database`, run `--migrate`, then start the runtime service. Role preparation runs directly in Rust through SQLx; it does not launch Python, Docker or psql. Credentials retain their existing schema, passwords and private file location. The normal role and grant policies have one SQL definition, shared with the transitional recovery tools.

Regular service startup verifies the compiled SQLx migration checksums and performs no schema DDL. Missing/mismatched history or a privileged runtime login prevents startup. Apply migrations before starting a new release; do not roll back only the executable after an incompatible migration. Complete native bundles record the read-only `ai-phone-database-prepare-v1` capability; older bundles must be rebuilt before using the new installer path.

For maintenance from the project root:

```sh
./target/debug/ai-phone-assistant --prepare-database
./target/debug/ai-phone-assistant --migrate
./target/debug/ai-phone-assistant --database-check
```

`prepare` is repeatable and preserves existing application passwords/data. A conflicting role without AI Phone's ownership marker aborts the transaction. It transfers only non-extension objects in this project's public schema, removes unexpected role memberships, and sets default privileges for new migration-owned objects. It never runs cluster-wide `REASSIGN OWNED`.

Tests create independent databases in an explicitly labelled test container. They cover empty-database migration, legacy ownership transfer, repeat execution, transaction refusal, business CRUD and denied table/role/history/password-table operations. Tests and production migration are separate evidence; neither establishes row-level tenant isolation. Current personal mode has one owner. SQL/maintenance access is a trusted administrator capability.

The native preparation regression runs with an empty PATH. The rest of installation/recovery orchestration and build/CI tools still contain Python and are being retired; this module does not establish a Python-free product or repository.
