# AI Phone

English · [简体中文](README.md#简体中文) · Apache-2.0 · **0.1.0-preview.2 experimental preview**

## Download and install

Download a preview client for your platform. **The macOS Apple Silicon client includes experimental local setup:** choose device resources, let the client acquire/build its dependencies, then configure a model in the workbench. Fresh-host installation remains untested. Windows, Linux and Android packages remain connection clients. The complete source host currently targets macOS Apple Silicon; Windows/Linux host management and Android independent hosting remain unfinished. See the [current release status](docs/public/release-status.md).

| System | Client format | Status / instructions |
|---|---|---|
| macOS Apple Silicon | [DMG](https://github.com/Conferringo/AI-Phone/releases/download/v0.1.0-preview.2/AI-Phone-0.1.0-preview.2-macos-arm64.dmg) | Experimental managed setup; fresh-host install untested; unsigned/unnotarized |
| Windows x64 | [EXE / NSIS](https://github.com/Conferringo/AI-Phone/releases/download/v0.1.0-preview.2/AI-Phone-0.1.0-preview.2-windows-x64-setup.exe) | Unsigned; installation/runtime untested |
| Linux amd64 / ARM64 | [amd64 DEB](https://github.com/Conferringo/AI-Phone/releases/download/v0.1.0-preview.2/AI-Phone-0.1.0-preview.2-linux-amd64.deb) / [arm64 DEB](https://github.com/Conferringo/AI-Phone/releases/download/v0.1.0-preview.2/AI-Phone-0.1.0-preview.2-linux-arm64.deb) | Ubuntu 24.04 builds; desktop/media untested |
| Android ARM64 | [APK](https://github.com/Conferringo/AI-Phone/releases/download/v0.1.0-preview.2/AI-Phone-0.1.0-preview.2-android-arm64.apk) | Preview certificate; touch/background untested |

[Open the release](https://github.com/Conferringo/AI-Phone/releases/tag/v0.1.0-preview.2) · [SHA256SUMS](https://github.com/Conferringo/AI-Phone/releases/download/v0.1.0-preview.2/SHA256SUMS). Releases include checksums, provenance manifests and third-party notices. Read the [preview scope and limits](docs/public/preview-release.md). The maintainer waived physical installation acceptance; untested platforms remain unverified. Report sanitized problems through [Issues](https://github.com/Conferringo/AI-Phone/issues/new/choose).

Source deployment requires Python 3.11+, Node.js 22+, npm, Rust 1.88+, Java 17, Android command-line tools, Xcode Command Line Tools and Docker Desktop. Recommended: 16GB RAM; reserve 35GiB for a fresh install.

```sh
./ai-phone doctor
./ai-phone install
```

Configuration asks for a language first, then device resources and prerequisites. Existing guest disks and accounts are preserved. Startup opens the workbench directly; first setup configures an Agent model and device bindings. External channels and memory are optional later settings. The project uses a pinned local Bun entry.

[Setup guide](docs/public/getting-started.en.md) · [Eight languages](docs/public/languages.md) · [Backup/restore](docs/public/backup-restore.en.md)

## Technical overview

AI Phone gives an Agent a real Android instance and a lightweight Linux desktop: observe screens, use applications, process files and keep a continuous conversation in one workbench. One phone and one computer are enabled by default. Conversations remain per device; devices may share an Agent or use separate models.

Actions rely on observations. Side effects carry user, device, run, control epoch and action IDs; unknown results are observed before retrying. Models can still misread interfaces. Execution records and independent checks make outcomes inspectable rather than promising perfect accuracy.

## Usage scenarios

- **Phone applications:** use installed apps such as WeChat, inspect actual notifications and prepare or send replies under the user's instructions. Account restrictions, login and application controls remain effective.
- **Phone/computer work:** receive a file on the phone, transfer it to Linux, process and inspect a spreadsheet, then return the result. File-transfer completion and delivery to a contact are distinct outcomes; external sending requires authorization.
- **Continuous tasks:** keep adding instructions and interrupt model generation. Messages persist; earlier text and operations remain visible; unknown tool outcomes are not replayed.
- **Reusable capability:** built-in phone-use, computer-use, transfer and application-learning Skills; linked knowledge documents; Agent-specific memory and external connections.
- **Guest networking:** native Clash applications run inside Android and Linux; users control guest networking there. Host networking defaults to direct access.

See the [compatibility matrix](docs/public/compatibility.md) for QQ, Feishu, WeCom and Telegram scope. Adapter code does not establish end-to-end acceptance for every account. The desktop companion has been removed from the core; only a [future extension design](docs/public/companion-extension.md) remains.

## Architecture and directories

| Layer | Implementation | Directory |
|---|---|---|
| Workbench | Next.js App Router/Turbopack, React, Tailwind, shadcn/Radix | `apps/web` |
| Native client | Tauri 2, managed macOS setup + local/LAN connection | `apps/client` |
| Agent/device services | Rust, Axum, Tokio | `crates` |
| Protocol/state | protobuf, PostgreSQL/SQLx; Redis for temporary coordination | `packages/protocol`, `compose.yaml` |
| Runtime/images | Native ARM64 Android Emulator, Docker Compose Linux | `infra`, `scripts` |
| Reusable capabilities | Skills, templates, version/provenance manifests | `skills`, `config` |
| Guides and evidence | Setup, architecture, privacy, audits, release scope | `docs/public` |

[Architecture](docs/public/architecture.en.md) · [Documentation index](docs/public/index.md) · [Conversation steering](docs/public/conversation-steering.md) · [Capabilities and data](docs/public/capabilities-and-data.md)

Models/APIs, connections and memory belong to Agents; Skills, knowledge and work files are shared. Code is separate from user state. Environment secrets, guest disks, accounts, chats, attachments and private evidence are excluded from the repository. Updates require versioned artifacts, backups, migrations and rollback rather than a direct `git pull` into user state.

## Roadmap

1. Clean-host installation, interrupted-install recovery, resource migration and signed updates.
2. Native platform input/audio/touch, language and accessibility acceptance; Windows/Linux host providers.
3. Smaller UI/execution ownership boundaries; long-running tasks and host sleep/reboot recovery.
4. Model discovery/capability probes, specialized app Skills and Skill upgrade/rollback.
5. Speech input/output, interruptible realtime voice, a call sandbox, then user-configured real telephony.

An emulator or eSIM screen does not supply a carrier number or cellular service. Telephony is not implemented. See the [roadmap](docs/public/release-roadmap.en.md) and [release status](docs/public/release-status.md) for explicit gates. Source hosting is distinct from complete product acceptance.

## Disclaimer and licensing

Provided as-is. Users control their devices, accounts, model services and channels and remain responsible for authorized operations and outcomes. Guest root is not arbitrary host access. Do not use the project for fraud, impersonation, unauthorized messaging/data access or other unlawful conduct.

Original code uses [Apache-2.0](LICENSE). Dependencies, SDKs, guest applications, assets and trademarks retain their own terms; the project licence does not grant binary redistribution rights for them. [Disclaimer](DISCLAIMER.md) · [Privacy](PRIVACY.md) · [Third-party notices](THIRD_PARTY_NOTICES.md) · [Security](SECURITY.md).

See [CONTRIBUTING](CONTRIBUTING.md) for development. Business APIs use protobuf; tests must be isolated and user state or live credentials must never be uploaded.

The maintainer elected to publish this experimental source preview without waiting for separate-device installation acceptance. Untested platforms remain unverified. Report problems through [Issues](https://github.com/Conferringo/AI-Phone/issues/new/choose); never attach credentials, private chats or device data. Binary release notices remain a separate requirement.
