# AI Phone

English · [简体中文](README.md#简体中文) · [繁體中文](docs/public/readme.zh-TW.md) · [日本語](docs/public/readme.ja.md) · [한국어](docs/public/readme.ko.md) · [Deutsch](docs/public/readme.de.md) · [Français](docs/public/readme.fr.md) · [Español](docs/public/readme.es.md) · [Русский](docs/public/readme.ru.md) · [العربية](docs/public/readme.ar.md)

An Android phone and a Linux desktop for your AI agents.

## Download

| Platform | Installer |
| --- | --- |
| macOS 14.4+ · Apple Silicon | [DMG 0.1.6](https://github.com/MrMilkDragon/AI-Phone/releases/download/v0.1.6/AI-Phone-0.1.6-macos-arm64.dmg) |
| Windows 11 · x64 | [EXE 0.1.6](https://github.com/MrMilkDragon/AI-Phone/releases/download/v0.1.6/AI-Phone-0.1.6-windows-x64-setup.exe) |

[GitHub Releases](https://github.com/MrMilkDragon/AI-Phone/releases) · [Build and platform status](docs/public/release-status.md)

Each installer includes the device images, emulator and local services. Configure devices and choose a model in the app. Setup and the workbench support ten [interface languages](docs/public/languages.md).

## What it does

- **Use a phone.** An Agent reads the Android screen, operates installed apps such as WeChat and handles notifications.
- **Use a computer.** An Agent controls the Linux desktop and works in apps such as LibreOffice. Connect the devices to move files between them.
- **Keep working.** Add instructions or interrupt a response in the same conversation. Each device keeps its conversation and activity while you switch views.
- **Reuse skills.** Agents share Skills, linked knowledge and work files. Model settings, memory and channel connections belong to each Agent.

See [model and channel setup](docs/public/model-and-channel-setup.md), [notification workflows](docs/public/notification-workflows.md) and [autonomous exploration](docs/public/autonomous-exploration.md).

## How it works

| Layer | Technology | Directory |
| --- | --- | --- |
| Workbench | Next.js App Router, TypeScript, Tailwind, shadcn/ui | `apps/web` |
| Native client | Tauri 2 | `apps/client` |
| Backend, devices and media | Rust, Axum, Tokio | `crates` |
| Business API and state | protobuf, PostgreSQL, SQLx | `packages/protocol` |
| Device runtimes | Android Emulator, Linux containers, Docker Compose | `infra`, `scripts` |
| Built-in capabilities | Skills and device profiles | `skills`, `config` |

Agents plan from current device observations and record their actions and results. Bun and Cargo build the project. Device disks, accounts and attachments stay in the local installation.

[Architecture](docs/public/architecture.en.md) · [Documentation](docs/public/index.md) · [Contributing](CONTRIBUTING.md) · [Report an issue](https://github.com/Conferringo/AI-Phone/issues/new/choose)

## Roadmap

Signed updates, voice Copilot, more app Skills and notification-driven office work. [Development roadmap](docs/public/release-roadmap.en.md)

## License and use

Original code is licensed under [Apache-2.0](LICENSE). Third-party components retain their own terms. Connect accounts and devices you are authorized to use.

[Disclaimer](DISCLAIMER.md) · [Privacy](PRIVACY.md) · [Third-party notices](THIRD_PARTY_NOTICES.md) · [Security](SECURITY.md)
