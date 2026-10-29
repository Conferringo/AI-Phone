# Release audit / 技术发布专项审计

2026-10-28 · local technical-preview candidate. This is an evidence-backed batch audit, not a claim that the entire project or every platform has been certified.

| Layer / 层 | Checked and fixed / 本轮完成 | Open boundary / 待完成 |
| --- | --- | --- |
| Frontend | Removed device “+” and duplicate legacy settings window/network page; actual screen-anchored toolbar; separated layout/camera/chat motion; no orientation capture when unchanged; eight language catalogs and shared windows; single-button entry; 390px/reduced-motion checks | Full language/backend-error, assistive technology and small-window matrix; user acceptance of motion |
| Rust backend | Authenticated protobuf smoke on current business service; pool/statement/lock/idle-transaction limits; service environment reload fixes ADB discovery; notification summaries coexist with paused user work | Provider/platform real-account matrix, host sleep/reboot, long-run fault recovery |
| Action execution | Durable sharded archive, bounded memory cache, no 2000-action stop; old and unknown records retained; single-runner lock; unsafe downgrade blocked | Whole-state backup and offline restored Android/Linux boot passed; automatic update/activation lifecycle remains open |
| PostgreSQL / SQLx | All original migration checksums matched; 0026 adds 15 lookup/FK indexes without modifying records; 24 applied migrations, no failed migrations/invalid indexes/unindexed FK columns/idle transactions; knowledge relation targets present | Production now uses a separate DML-only runtime role, explicit owner/migration login and preserved bootstrap administrator. Empty/legacy/denial/rollback tests passed; single-owner schema is not multi-tenant RLS |
| protobuf | Buf STANDARD lint/build; 12 TypeScript generated files regenerate byte-identically; Rust prost build; actual authenticated registry round trip and invalid wire/media/auth checks | No previously published release baseline exists; establish a descriptor baseline at the first public tag |
| Redis | Explicit optional cache profile; isolated pinned Redis 8: PING, AOF restart, 128MB noeviction, no published ports, test keys removed | Not currently running in the product and not a business queue/cache. PostgreSQL is the actual queue; do not advertise Redis distribution as implemented |
| Guest apps/network | Native Clash Meta/Clash Party installed; native subscription handling, TUN, browser/download and guest restart checks; empty Linux image checked without private configuration | Full region/subscription/FCM/UDP and host lifecycle coverage |
| Distribution | Source allowlist, private-value scan, SHA256/manifest, app/user separation and staging tests; public package refuses open gates | Clean physical-host install, project/EPIC asset licences, signed four-platform clients and complete updater |

Verification: 25 phone-runner tests; 54 regular assistant tests plus explicitly run isolated owner-entry and notification-cycle database tests; frontend/network/installer regressions; production build/typecheck; live native networking, archive replay and UI evidence. Ignored tests are not counted as passed. Existing 297 message rows were compared individually and remained unchanged; new notification summaries are additional messages.

The PostgreSQL superuser finding was fixed through a transactional, tested ownership/privilege migration, preserving the bootstrap administration account. The Redis check used an isolated fixture instance and did not access other projects. Test scripts no longer automatically cancel a paused user task; service-disruption checks require an explicit flag.

`config/release-policy.json` remains authoritative. Action retention now has concrete passing evidence; clean physical-host installation, complete UI, licensing and long-running recovery still block public packaging. No global “95% ready” claim is supported by these remaining unmeasured gates. Four-platform dependency-free one-click installation has not been delivered.

## Database/install/recovery follow-up

Current instance: 300 messages, one owner and the original paused task retained. Three dedicated role tests and six archive/restore tests passed, including rollback of imported schema/data on role refusal. Full state (12.44 GB, 3,019 files) was restored: all 33 table fingerprints matched; Linux GUI/root/browser/office/Clash/TUN and 10 work files passed; Android restored boot/root/real screenshot/WeChat/Clash passed offline. Test images and copies were cleaned up.

The same-host source-envelope test creates its own node_modules and target, compiles protobuf/TypeScript/Rust and production Web, and initializes an empty database with 24 SQLx migrations and 7 built-in Skills. It does not start production devices or assert a clean-machine installation. The default 16 GB fresh-phone boot was refused correctly for insufficient temporary space; a smaller blank fixture was used only to establish offline test networking.

Fixed defects exposed by real tests: Turbopack followed private user links outside the version source root (runtime-only read now ignored by bundler tracing); runtime sockets broke state archiving; screenshot-row aggregate fingerprinting exhausted a 256 MB PG fixture (streaming hashes passed); path replacement doubled nested restore roots (one-pass, idempotent longest-path rewrite). Directory envelopes and streaming restore reduce temporary copies; recovery writes/imports preserve 4 GiB free.

References for the implementation choices: [Next.js server runtime fs usage](https://github.com/vercel/next.js/blob/canary/packages/next/src/server/next-server.ts) and [QEMU restricted user networking](https://www.qemu.org/docs/master/system/invocation.html). Real test results, rather than option names or netcat scan exit codes, establish offline behavior.

## Worker and install interruption follow-up

Fixed a claimed-run orphan window during PostgreSQL outages: claim includes the message ID; task/message/top-level event finalization is atomic and retried without replaying tools. Unknown in-flight results pause. Actual worker subprocess faults, PG stop/start, incomplete SSE, quota/backoff and 56-turn tool progression passed, alongside all 8 isolated DB integration tests. Default startup now restores only missing owned jobs; a live LAN-only removal/recovery preserved all other six PIDs. Private install checkpoints validate source and artifact identity; 9 interruption/ownership cases passed. Host sleep/reboot and physical clean installation remain open. See [recovery evidence](task-recovery.md).
