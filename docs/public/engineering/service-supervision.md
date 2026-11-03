# Native service supervision

Linux and the WSL backend run their application services under the Rust supervisor in `ai-phone-host-runtime`. The packaged assistant exposes this lifecycle independently of its model server; supervision does not connect to the database or initialize an Agent. Each supervisor uses a single-thread Tokio runtime and waits for child exit or signals rather than polling a live child.

## Installation contract

The native component inventory declares `ai-phone-linux-supervision-v1`. Packaging qualifies the executable through its read-only `--service-supervision-contract` entry. Installation selects the matching verified assistant executable before starting a service. Source installations qualify their newly built executable separately.

The installation controller retains its existing `data/services/<name>.lock` while publishing the command and environment specification. It transfers that same open lock to the supervisor as standard input. The supervisor checks the file identity and ownership, verifies the exact specification bytes, and publishes its PID, process start time, executable, root and specification identities atomically. Worker standard input is `/dev/null`; workers do not inherit the supervisor's lock.

The canonical specification fingerprint remains the identity used for resume and configuration conflicts. A second digest binds the serialized bytes during startup. This preserves the existing stored identity without reproducing another language's JSON serialization rules in the Rust process owner.

## Process lifetime

The Rust owner launches each worker in its own process group with the configured working directory and environment. Child exit schedules a two-second restart delay. SIGTERM and SIGINT interrupt either normal execution or the restart delay. Shutdown first sends SIGTERM to the owned worker group, retains the existing five-second grace period, and reaps a resistant child after SIGKILL. The controller keeps its eight-second stop deadline.

State inspection checks UID, PID start time, executable and complete invocation before accepting a native receipt. Existing Python receipts remain inspectable by the installation adapter. Stale receipts do not authorize signalling a reused PID, and a live service with a different specification is preserved with an explicit configuration conflict.

The Python adapter still performs installation control and state inspection; the persistent supervision loop is Rust. Hard termination of the supervisor and host reboot require the installation recovery path and are separate from child-exit recovery. Service receipts do not authorize replaying device actions or establish full device readiness.
