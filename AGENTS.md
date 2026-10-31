# Coding agent instructions

- Use Next.js App Router/Turbopack, Tailwind/shadcn; Rust/Axum/Tokio; PostgreSQL/SQLx; Bun/Cargo. Business APIs use protobuf. Update protocol sources before generating TypeScript/Rust.
- Run `scripts/bootstrap.sh` first and use `scripts/bun`. Do not reuse another project's dependencies or build artifacts.
- Treat `data/`, `.env`, user storage, guests, account credentials, conversations, paired channels and audit evidence as private. Never commit or upload them. Work only on explicitly owned services and test containers.
- Preserve guest disks, account data, action IDs, control epochs, model revisions and unknown outcomes. Unknown side effects must be checked before retrying. Guest root is not arbitrary host access.
- Follow `docs/public/ui-system.md`. One phone and one computer by default, device-specific conversations, one configuration entry. Keep working inputs, drafts, visible errors, keyboard focus and reduced motion.
- Development tests must be isolated. Scripts that touch devices/models require an explicitly designated test environment; do not interrupt an active user task. CI uses disposable fixtures and newly created virtual devices, never personal devices, social accounts or live model credentials.
- Keep versioned code separate from user state. Test migrations and restore on disposable databases before changing an existing installation. Never reset a checkout or prune Docker globally.
- Update compatibility and both language setup guides when capabilities change. Plans are not passed tests. Distinguish source preview, reproducible same-host builds, clean physical-host installation and production acceptance.
