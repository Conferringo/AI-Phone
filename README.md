# AImanity

English · [简体中文](#简体中文) · [繁體中文](docs/public/readme.zh-TW.md) · [日本語](docs/public/readme.ja.md) · [한국어](docs/public/readme.ko.md) · [Deutsch](docs/public/readme.de.md) · [Français](docs/public/readme.fr.md) · [Español](docs/public/readme.es.md) · [Русский](docs/public/readme.ru.md) · [العربية](docs/public/readme.ar.md)

An agent that listens while it works, explores on its own, and learns from using your phone and computer.

## 0.0.1 Aluminum

Get the source for the first AImanity release. [Releases](https://github.com/MrMilkDragon/AImanity/releases) · [macOS and Windows build instructions](docs/public/release-status.md)

## What it does

- **Use a phone.** An Agent reads the Android screen, operates installed apps such as WeChat and handles notifications.
- **Use a computer.** An Agent controls the Linux desktop and works in apps such as LibreOffice. Connect the devices to move files between them.
- **Keep working.** Add instructions or interrupt a response in the same conversation. One conversation and one selected Agent follow you across both devices.
- **Choose a mode.** Copilot takes live voice instructions. Liberty explores freely. Task & Goal works toward a goal you can revise as it runs.
- **Reuse skills.** Skills, linked knowledge and work files are shared. Save model profiles and switch the active Agent in Configuration.

See [model and channel setup](docs/public/model-and-channel-setup.md), [notification workflows](docs/public/notification-workflows.md) and [Liberty](docs/public/autonomous-exploration.md).

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

[Architecture](docs/public/architecture.en.md) · [Documentation](docs/public/index.md) · [Contributing](CONTRIBUTING.md) · [Report an issue](https://github.com/MrMilkDragon/AImanity/issues/new/choose)

## Roadmap

More voice providers, app Skills and notification-driven office workflows. [Development roadmap](docs/public/release-roadmap.en.md)

## License and use

Original code is licensed under [Apache-2.0](LICENSE). Third-party components retain their own terms. Connect accounts and devices you are authorized to use.

[Disclaimer](DISCLAIMER.md) · [Privacy](PRIVACY.md) · [Third-party notices](THIRD_PARTY_NOTICES.md) · [Security](SECURITY.md)

---

# 简体中文

[Back to English](#aimanity)

## 0.0.1 Aluminum

获取 AImanity 首发源码。[发行页](https://github.com/MrMilkDragon/AImanity/releases) · [macOS 与 Windows 构建](docs/public/release-status.md)

## 功能

- **操作手机。** Agent 查看真实 Android 画面，操作微信等已安装应用并处理通知。
- **操作电脑。** Agent 控制 Linux 桌面，可使用 LibreOffice 等应用。连接两台设备即可互传文件。
- **持续工作。** 在同一对话中补充指令或打断回复。切换设备时，同一 Agent 和对话继续工作。
- **选择模式。** Copilot 接收实时语音，Liberty 自由探索，任务与目标支持边执行边修订。
- **复用能力。** Skill、关联知识文档和工作文件共享。在配置中心保存模型配置并切换当前 Agent。

[模型与渠道](docs/public/model-and-channel-setup.md) · [通知工作流](docs/public/notification-workflows.md) · [Liberty](docs/public/autonomous-exploration.md)

## 技术架构

工作台采用 Next.js、TypeScript、Tailwind 和 shadcn/ui，客户端采用 Tauri 2。后端、设备控制和媒体使用 Rust、Axum 和 Tokio。业务接口使用 protobuf，数据由 PostgreSQL 和 SQLx 管理。使用 Bun 和 Cargo 构建，Android Emulator 和 Docker Compose 运行设备与服务。

Agent 根据当前设备画面操作，动作账本保留执行过程和结果。设备盘、账号、聊天和附件保存在本地安装目录或数据库中。

[架构与目录](docs/public/architecture.zh-CN.md) · [文档](docs/public/index.md) · [参与贡献](CONTRIBUTING.md) · [提交问题](https://github.com/MrMilkDragon/AImanity/issues/new/choose)

## 后续开发

扩展语音服务商、应用专项 Skill 和通知触发的办公流程。[开发路线](docs/public/release-roadmap.zh-CN.md)

## 许可与使用

原创代码采用 [Apache-2.0](LICENSE)。第三方组件沿用各自许可。请仅连接有权使用的账号和设备。

[免责声明](DISCLAIMER.md) · [隐私](PRIVACY.md) · [第三方声明](THIRD_PARTY_NOTICES.md) · [安全](SECURITY.md)
