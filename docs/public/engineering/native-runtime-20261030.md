# Native runtime review — 2026-10-30

Scope: native build provenance, component inventory, archive admission, launch selection and isolated execution. The active package contract is [Runtime bundles](../runtime-bundles.md). This review does not certify complete installers or whole-repository SOLID compliance.

## Ownership and contracts

- `build_native_services.py` owns host-matched Cargo/Swift output and its source fingerprint. `macos_target.py` reads the client deployment target; compiled helpers must not require a newer macOS.
- `prebuilt_services.py` owns component inventory and launch selection. Invalid installed components fail explicitly. Source-only contributor startup remains supported.
- `client_host_entry.py` owns bounded archive admission, file integrity and persistent user links. Component declarations cannot admit files owned by an undeclared component.
- `runtime_verification.py` owns the minimum subprocess environment. `check_native_services.py` owns generated-image/version checks and admission of labelled, loopback-only test databases.
- macOS/Linux startup selects verified binaries before performing device setup. Rust and Swift compilation are contributor fallback paths. The runner receives the matching OCR helper.

Source installation retains ordered checkpoints and explicit installation roots. SDK, application dependencies and guest image preparation have separate functions. Linux startup separates guest readiness, execution services and workbench services; active units are reused. Existing-profile tests ensure device resources are not replaced by a later setup request.

## Verification

The local macOS App/DMG contains the prebuilt web and native components. Files were taken from the final App resources, staged into an empty temporary directory and executed: HTTP page, 13 script hashes, PNG bytes, both native version commands and OCR digits `7319`. Native version checks created no state. Mach-O minimum versions were checked against the client's macOS 14.4 requirement.

The actual release assistant applied 29 embedded migrations to a dedicated temporary PostgreSQL instance. Its runtime account could read the migration ledger and could not create a table. Label, container name and exact loopback binding are checked before the database fixture is used or cleaned up. Production devices and database are excluded from these tests.

Local Python regression: 133 tests, with 14 explicit environment skips. Rust: 84 assistant and 30 runner unit tests passed; the ordinary assistant run reports 31 ignored environment/live tests. Strict workspace Clippy and formatting passed. The native database exercise above is separate from that ordinary suite. Matching platform CI remains necessary for every published revision.

A targeted mutation run executed all 53 sites in `prebuilt_services.py` and `macos_target.py`; all were killed. Coverage filtering was disabled for this run so a continued dictionary-expression line could not be skipped by the line tracer. This is a two-module mutation result, not whole-system mutation coverage.

Dryer reported one 0.80 similarity candidate between Node and Docker prerequisite probes. It was retained: the commands, outputs and failure contracts differ, and the shared result format already has one owner. No second probe policy was introduced to satisfy a structural metric.

## UI and model boundary

The live configuration page showed all 13 provider presets. Text fields and custom selects measured 40px height and 8px radius. At 375px and 320px widths the window stayed inside the viewport without horizontal overflow. Switching to knowledge closed the old window; returning preserved an unsaved Agent draft. The draft was restored without saving. Closing also worked with reduced motion.

These checks did not change production model settings or call paid providers. Full window/language/accessibility coverage and account-level channel acceptance remain separate requirements. The [compatibility matrix](../compatibility.md) records supported protocols and actual evidence.

## Remaining work

Complete guest images, emulator/VM runtime, media/channel/browser dependencies, redistribution notices, Windows/Android host integration and first-run/update activation remain open. Linux OCR currently needs system Tesseract/Pillow dependencies. The local full-install disk prerequisite is not met; recovery space protections remain enforced. Large contributor CLI, container-engine, UI and execution boundaries remain in the [SOLID audit](../solid-audit.md).
