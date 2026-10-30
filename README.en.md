# AI Phone

English · [简体中文](README.md#简体中文) · [繁體中文](docs/public/readme.zh-TW.md) · [日本語](docs/public/readme.ja.md) · [한국어](docs/public/readme.ko.md) · [Deutsch](docs/public/readme.de.md) · [Français](docs/public/readme.fr.md) · [Español](docs/public/readme.es.md) · [Русский](docs/public/readme.ru.md) · [العربية](docs/public/readme.ar.md)

An Android phone and a Linux desktop for your AI agents.

Apache-2.0 · Local deployment · Ten interface languages

## Downloads

**Complete installers are not available yet.** Package availability and supported host configurations are listed below. Source code, architecture and configuration guides are available in this repository.

Installation flow: **device resources → model → workbench**. A complete package includes matched Android/Linux system images, emulator and prebuilt services. Device creation and runtime setup happen inside the client; users do not prepare an SDK or compile a backend.

| Platform | Package | Availability |
|---|---|---|
| macOS Apple Silicon | Desktop application with local device runtime | Preparing the complete package |
| Windows x64 | Desktop application with local device runtime | Host integration in progress |
| Linux x64 | Desktop application with local device runtime | Preparing the complete package |
| Android ARM64 | Root/KVM application with local device runtime | Privileged runtime integration in progress |

[Release requirements](docs/public/preview-release.md) · [Current status](docs/public/release-status.md) · [Issues](https://github.com/Conferringo/AI-Phone/issues/new/choose)

For source builds, see the contributor guides. Distribution images contain clean system baselines; device data, accounts, conversations, attachments and credentials stay in the user directory.

## Technical overview

AI Phone gives an Agent a real Android instance and a lightweight Linux desktop: observe screens, use applications, process files and keep a continuous conversation in one workbench. One phone and one computer are enabled by default. Conversations remain per device; devices may share an Agent or use separate models.

Actions rely on observations. Side effects carry user, device, run, control epoch and action IDs; unknown results are observed before retrying. Models can still misread interfaces. Execution records and independent checks make outcomes inspectable rather than promising perfect accuracy.

Interface and first-setup languages: all ten languages above. The language control sits immediately left of Skills; Arabic supports right-to-left text. [Language scope](docs/public/languages.md).

## Usage scenarios

- **Phone applications:** use installed apps such as WeChat, inspect actual notifications and prepare or send replies under the user's instructions. Account restrictions, login and application controls remain effective.
- **Phone/computer work:** receive a file on the phone, transfer it to Linux, process and inspect a spreadsheet, then return the result. File-transfer completion and delivery to a contact are distinct outcomes; external sending requires authorization.
- **Continuous tasks:** keep adding instructions and interrupt model generation. Messages persist; earlier text and operations remain visible; unknown tool outcomes are not replayed.
- **Reusable capability:** built-in phone-use, computer-use, transfer and application-learning Skills; linked knowledge documents; Agent-specific memory and external connections.
- **Guest networking:** native Clash applications run inside Android and Linux; users control guest networking there. Host networking defaults to direct access.

See the [compatibility matrix](docs/public/compatibility.md) for QQ, Feishu, WeCom, Telegram and local-QR WhatsApp scope. Adapter code does not establish end-to-end acceptance for every account. [Model and connection setup](docs/public/model-and-channel-setup.md) covers all 13 presets and the three API protocols.

## Architecture and directories

| Layer | Implementation | Directory |
|---|---|---|
| Workbench | Next.js App Router/Turbopack, React, Tailwind, shadcn/Radix | `apps/web` |
| Native client | Tauri 2, managed macOS setup + local/LAN connection | `apps/client` |
| Agent/device services | Rust, Axum, Tokio | `crates` |
| Protocol/state | protobuf, PostgreSQL/SQLx; Redis reserved for optional caching | `packages/protocol`, `compose.yaml` |
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

Platform build and runtime evidence is documented in the compatibility matrix. Non-macOS physical installation tests are not required for this release scope and remain unverified. Report problems through [Issues](https://github.com/Conferringo/AI-Phone/issues/new/choose); never attach credentials, private chats or device data.

---

