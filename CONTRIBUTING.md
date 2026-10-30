# Contributing / 参与贡献

This is a local source-preview candidate. Original project code is Apache-2.0; dependencies/assets retain their own terms. No GitHub upload is authorized in this batch. Do not upload a development checkout's private state when preparing a contribution.

## Development

Use the documented stack and project-local Bun. Read [the public UI system](docs/public/ui-system.md) (and DESIGN.md in a full development checkout) for UI decisions and the protobuf definitions before changing an endpoint. Keep default device limits at one phone and one computer. Preserve device ownership, action IDs, control epochs and existing conversation state.

```sh
./scripts/bootstrap.sh
./scripts/bun run check
./scripts/bun test ./apps/channels ./scripts/locale.test.ts ./scripts/device-focus.test.ts
python3 -m unittest discover -s scripts -p test_release.py
cargo fmt --all --check
cargo test --workspace --locked
./scripts/bun run build
```

`scripts/verify-*` can control real devices or call a paid model. Inspect a script first and run it only against an explicitly designated idle test environment. Unit tests using mocks are not real platform certification.

## Changes

Follow SOLID at real ownership boundaries: separate domain policy from transport/persistence, give each mutable resource one lifecycle owner, and preserve cancellation, error and unknown-outcome contracts across adapters. Avoid speculative base classes and wrapper layers. Pin behavior before refactoring and reproduce defects before fixing them. See the [SOLID audit and remaining work](docs/public/solid-audit.md); the current repository is not yet a clean-code certification. Do not suppress diagnostics or weaken tests to claim completion.

Explain the problem, resulting behavior, tests and remaining limits. Update both language guides and the compatibility matrix when support changes. UI labels belong in the shared locale dictionary. Keep errors visible, drafts persistent and controls keyboard-accessible. Do not add disconnected buttons or duplicate configuration entrances.

App-experience contributions must strip accounts, contacts, file names, private URLs, screenshots, run IDs and subscription details. Record tested app/version limitations rather than presenting a single successful run as a universal recipe. Guest proxy binaries require pinned provenance and a complete licence review.

Source packages are built with `./ai-phone package`. Do not use a blanket archive or upload the entire workspace. Public packaging additionally requires completed publication gates and a manual review of the resulting archive.

GitHub workflow definitions are prepared with pinned checkout/setup-node commits and read-only repository permissions. They run only mocks and isolated databases, never a real phone or paid provider. Remote CI has not yet run because the repository is not uploaded.
