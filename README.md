# AI Phone

English · [简体中文](#简体中文) · [繁體中文](docs/public/readme.zh-TW.md) · [日本語](docs/public/readme.ja.md) · [한국어](docs/public/readme.ko.md) · [Deutsch](docs/public/readme.de.md) · [Français](docs/public/readme.fr.md) · [Español](docs/public/readme.es.md) · [Русский](docs/public/readme.ru.md) · [العربية](docs/public/readme.ar.md)

An Android phone and a Linux desktop for your AI agents.

## Download

| Platform | Installer |
| --- | --- |
| macOS 14.4+ · Apple Silicon | [DMG 0.1.3](https://github.com/MrMilkDragon/AI-Phone/releases/download/v0.1.3/AI-Phone-0.1.3-macos-arm64.dmg) |
| Windows 11 · x64 | [EXE 0.1.3](https://github.com/MrMilkDragon/AI-Phone/releases/download/v0.1.3/AI-Phone-0.1.3-windows-x64-setup.exe) |

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

---

# 简体中文

[Back to English](#ai-phone)

## 下载

| 平台 | 安装包 |
| --- | --- |
| macOS 14.4+ · Apple Silicon | [DMG 0.1.3](https://github.com/MrMilkDragon/AI-Phone/releases/download/v0.1.3/AI-Phone-0.1.3-macos-arm64.dmg) |
| Windows 11 · x64 | [EXE 0.1.3](https://github.com/MrMilkDragon/AI-Phone/releases/download/v0.1.3/AI-Phone-0.1.3-windows-x64-setup.exe) |

[GitHub Releases](https://github.com/MrMilkDragon/AI-Phone/releases) · [构建和平台状态](docs/public/release-status.md)

完整安装包内置设备镜像、模拟器和服务。打开客户端，配置设备和模型后进入工作台。初次配置和工作台支持[十种界面语言](docs/public/languages.md)。

## 功能

- **操作手机。** Agent 查看真实 Android 画面，操作微信等已安装应用并处理通知。
- **操作电脑。** Agent 控制 Linux 桌面，可使用 LibreOffice 等应用。连接两台设备即可互传文件。
- **持续工作。** 在同一对话中补充指令或打断回复。切换设备时，各自的聊天和任务继续保留。
- **复用能力。** Skill、关联知识文档和工作文件共享。模型、记忆和外部连接按 Agent 配置。

[模型与渠道](docs/public/model-and-channel-setup.md) · [通知工作流](docs/public/notification-workflows.md) · [自主探索](docs/public/autonomous-exploration.md)

## 技术架构

工作台采用 Next.js、TypeScript、Tailwind 和 shadcn/ui，客户端采用 Tauri 2。后端、设备控制和媒体使用 Rust、Axum 和 Tokio。业务接口使用 protobuf，数据由 PostgreSQL 和 SQLx 管理。使用 Bun 和 Cargo 构建，Android Emulator 和 Docker Compose 运行设备与服务。

Agent 根据当前设备画面操作，动作账本保留执行过程和结果。设备盘、账号、聊天和附件保存在本地安装目录或数据库中。

[架构与目录](docs/public/architecture.zh-CN.md) · [文档](docs/public/index.md) · [参与贡献](CONTRIBUTING.md) · [提交问题](https://github.com/Conferringo/AI-Phone/issues/new/choose)

## 后续开发

推进签名更新、语音 Copilot、应用专项 Skill 和通知触发的办公流程。[开发路线](docs/public/release-roadmap.zh-CN.md)

## 许可与使用

原创代码采用 [Apache-2.0](LICENSE)。第三方组件沿用各自许可。请仅连接有权使用的账号和设备。

[免责声明](DISCLAIMER.md) · [隐私](PRIVACY.md) · [第三方声明](THIRD_PARTY_NOTICES.md) · [安全](SECURITY.md)
