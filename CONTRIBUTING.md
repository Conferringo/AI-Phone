# Contributing / 参与贡献

Original project code is Apache-2.0; dependencies and assets retain their own terms. Keep device disks, accounts, conversations, attachments, credentials and local audit evidence out of contributions.

## Development

Use the documented stack and project-local Bun. Read [the public UI system](docs/public/ui-system.md) (and DESIGN.md in a full development checkout) for UI decisions and the protobuf definitions before changing an endpoint. Keep default device limits at one phone and one computer. Preserve device ownership, action IDs, control epochs and existing conversation state.

```sh
./scripts/bootstrap.sh
./scripts/bun run check
./scripts/bun test ./apps/channels ./apps/device-network ./apps/device-stream ./scripts/*.test.ts
python3 -m unittest discover -s scripts -p 'test_*.py'
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

`master` requires a pull request and passing Source, Native and WSL backend qualification checks from GitHub Actions, with the branch up to date. Administrators follow the same rule. Finish validation of one candidate before advancing the next; retain failures and fix their cause rather than weakening a check. The backend qualification builds and boots the bundled Linux runtime before merge; it does not qualify a Windows installation.

Workflows use pinned actions and read-only repository permissions. Automated tests use mocks and isolated databases; they do not control personal devices or call paid providers. Dependency audit failures name the affected package and advisory; the Native workflow retains a sanitized report for diagnosis. Passing source and native-build checks does not qualify a complete client release: full runtime resources, licensing, privacy and the publication gates are checked separately.
