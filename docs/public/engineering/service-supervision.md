# Native service supervision

Linux and the WSL backend run their application services under the Rust supervisor in `ai-phone-host-runtime`. The packaged assistant exposes this lifecycle independently of its model server; supervision does not connect to the database or initialize an Agent. Each supervisor uses a single-thread Tokio runtime and waits for child exit or signals rather than polling a live child.

## Installation contract

The native component inventory declares `ai-phone-linux-supervision-v2`, covering both the controller and the persistent supervisor. Packaging qualifies the executable through its read-only `--service-supervision-contract` entry. Installation selects the matching verified assistant executable before starting a service. Source installations first build and qualify the independent `host-resources` executable, before starting even the emulator. Subsequent assistant compilation cannot replace a running supervisor executable; resuming an existing installation reuses its qualified supervisor.

The installation controller retains its existing `data/services/<name>.lock` while publishing the command and environment specification. It transfers that same open lock to the supervisor as standard input. The supervisor checks the file identity and ownership, verifies the exact specification bytes, and publishes its PID, process start time, executable, root and specification identities atomically. Worker standard input is `/dev/null`; workers do not inherit the supervisor's lock.

The Rust controller owns start, state inspection and stop. New specifications use one computed byte identity in both existing receipt slots. Earlier Rust supervisor receipts keep their recorded identities and PID; matching bound specifications are compared as typed values without rewriting their serialized specifications. Conflicting or unverifiable configuration is rejected. Receipt and conflict inspection retain their 8 KiB budget; the supervisor's execution specification remains bounded at 1 MiB.

## Process lifetime

The Rust owner launches each worker in its own process group with the configured working directory and environment. Child exit schedules a two-second restart delay. SIGTERM and SIGINT interrupt either normal execution or the restart delay. Shutdown first sends SIGTERM to the owned worker group, retains the existing five-second grace period, and reaps a resistant child after SIGKILL. The controller keeps its eight-second stop deadline.

State inspection checks UID, PID start time, executable and complete invocation before accepting a native receipt. Stale or unverified process receipts do not authorize signalling. Existing files are preserved. A live Rust service with a different specification is preserved with an explicit configuration conflict; diagnostics report only fixed environment-field names and change counts.

Native library and real-process tests cover start/resume, ownership, older receipt encodings, child-exit recovery, backoff and shutdown. Current-source qualification is tracked separately from previous component evidence.

Hard termination of the supervisor and host reboot require the installation recovery path and are separate from child-exit recovery. Service receipts do not authorize replaying device actions or establish full device readiness.

Source launchers delegate startup order, database preparation, token creation and service specifications to `--source-services` in the Rust host component. `scripts/phone-lab-start.sh` and `scripts/phone-lab-stop.sh` are thin native command launchers. TypeScript tooling collects contributor input and builds resources; it does not supervise runtime processes.
