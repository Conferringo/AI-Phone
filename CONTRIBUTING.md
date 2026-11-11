# Contributing

Keep credentials, chats, device disks, attachments and local audit evidence out of commits and issue reports.

## Development

Use the project Bun wrapper and the pinned Rust toolchain.

```sh
./scripts/bootstrap.sh
./scripts/bun install --frozen-lockfile
./scripts/bun run check
cargo fmt --all --check
cargo test --workspace --locked
./scripts/bun run build
```

Run device and provider checks only in a designated test environment. The [workflows](.github/workflows) define platform-specific checks.

## Submit a change

Keep domain policy separate from transport and persistence. Preserve device ownership, action IDs, control epochs and existing state. Start API changes in the [protobuf schema](packages/protocol/proto). Use the shared locale dictionary and [UI system](docs/public/ui-system.md) for interface changes.

Describe the resulting behavior and how you verified it. For an app Skill, name the app version and the steps you exercised. Remove account data and private material from examples.

Refresh the public source manifest before submitting.

```sh
./scripts/bun scripts/source.ts manifest
./scripts/bun scripts/source.ts check --repository --manifest
```

Open a pull request against `master`. Required checks cover source, macOS Apple Silicon, Windows x64, Android guest helpers and the Linux backend. PR builds use read-only repository access and public build inputs. GitHub squash-merges accepted PRs.

Run the complete DMG or EXE through the manual installer factory in a private build repository. See the [architecture](docs/public/architecture.en.md) and [distribution requirements](docs/public/distribution-requirements.md).
