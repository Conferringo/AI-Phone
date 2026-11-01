# Emulator source producer

This maintainer entry synchronizes a fixed AOSP source selection and performs one native Emulator build. It is separate from client installation. Complete-client publication remains gated on boot, dependency source and redistribution qualification.

The [source lock](source-lock.xml) contains 75 immutable project revisions derived from the official Emulator build 12560773 manifest. Linux selects 66 projects. The producer verifies each selected checkout and fourteen build-recipe blobs before invoking the pinned build entry. Its own identity is `aiphone-source-d1bfd6eb0ea9-r1`; this selection declares Emulator 35.3.8. It is not corresponding-source evidence for later Google SDK binaries.

Synchronization uses partial cloning and separates network fetch from worktree checkout. The Linux Clang project expands the recipe's pinned `clang-r487747c` tree and root files, including notices, while leaving unused compiler versions unexpanded. All 66 selected projects and their commit identities remain in the lock. Both synchronization phases share the original time budget; source verification and build qualification remain separate.

The manually dispatched **Linux Emulator source build** workflow runs isolated unit contracts and real process-ownership regression first. On a disposable GitHub-hosted Linux runner, it verifies the pinned repo tag and launcher, synchronizes source and builds once. Only the fixed-field result receipt is uploaded. Binaries and private build logs are excluded. Interrupted work remains unknown and is not automatically replayed.

For a failed compilation, the receipt retains the verified producer's exit code and bounded diagnostic categories from the last 64 KiB of each build log. Traceback locations are limited to the pinned recipe files; exception messages, arbitrary paths, command lines and log contents are excluded. Missing or malformed diagnostic inputs preserve the original failure.

Run `python3 -B -m unittest -v test_aosp_emulator_producer.py test_linux_bootstrap.py` in this directory for source-contract tests. `qualify_owned_session.py` is a Linux-only regression using owned child processes. It does not access a device, network or source checkout. A real source build requires a fresh dedicated build host with sufficient resources; it is not intended for an active user-data directory.

Exact test-input attribution is in [fixture provenance](fixtures/provenance.json). The [official manifest](https://storage.googleapis.com/android-build/builds/aosp-emu-master-dev-linux-emulator-linux_x64_gfxstream/12560773/f865811d50184826de27f37db731130c9bdbeac24915ec0cde35704959e366ff/manifest_12560773.xml) has SHA-256 `8488ef05b46716de28349878c4bad4dbec3ee47fa77a2e643394be14633fa856`; this lock has SHA-256 `be4339c025820be0bdf15e6d21eb9a0c83e23c8d8b8e520bc3de5bd6d8b74373`. Tests read or parse upstream inputs without importing them.
