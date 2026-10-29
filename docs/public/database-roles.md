# Database roles / 数据库权限

AI Phone uses three separate PostgreSQL logins:

| Login | Purpose | Rights |
| --- | --- | --- |
| Bootstrap account from private `.env` | Create/repair roles, backup and restore | Database administrator; never used by the assistant |
| `ai_phone_migrator` | Explicit SQLx migrations | Owns application tables; schema/database CREATE, no superuser or role-management rights |
| `ai_phone_runtime` | Assistant service | SELECT/INSERT/UPDATE/DELETE on business tables, sequence access; schema history SELECT only |

Passwords for the two application logins are generated once in `data/database/credentials.json` with mode 0600. Keep this file and `.env` in private backups. They never enter the browser, protobuf response, source bundle or command-line arguments. The bootstrap account is retained for recovery; the migration does not rename, demote or replace it.

`assistant-start.sh` prepares roles transactionally, runs `ai-phone-assistant --migrate`, then starts the runtime service. Regular startup only verifies the compiled SQLx migration checksums; it performs no schema DDL. Missing/mismatched history or a privileged runtime login prevents startup. Apply migrations before starting a new release; do not roll back only the executable after an incompatible migration.

For maintenance from the project root:

```sh
python3 scripts/database.py prepare
./target/debug/ai-phone-assistant --migrate
./target/debug/ai-phone-assistant --database-check
```

`prepare` is repeatable and preserves existing application passwords/data. A conflicting role without AI Phone's ownership marker aborts the transaction. It transfers only non-extension objects in this project's public schema, removes unexpected role memberships, and sets default privileges for new migration-owned objects. It never runs cluster-wide `REASSIGN OWNED`.

Tests create independent databases in an explicitly labelled test container. They cover empty-database migration, legacy ownership transfer, repeat execution, transaction refusal, business CRUD and denied table/role/history/password-table operations. Tests and production migration are separate evidence; neither establishes row-level tenant isolation. Current personal mode has one owner. SQL/maintenance access is a trusted administrator capability.
