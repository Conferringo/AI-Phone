# AI Phone

English · [简体中文](#简体中文) · [繁體中文](docs/public/readme.zh-TW.md) · [日本語](docs/public/readme.ja.md) · [한국어](docs/public/readme.ko.md) · [Deutsch](docs/public/readme.de.md) · [Français](docs/public/readme.fr.md) · [Español](docs/public/readme.es.md) · [Русский](docs/public/readme.ru.md) · [العربية](docs/public/readme.ar.md)

Apache-2.0 · **Development — distribution in preparation**

## Downloads

There is currently **no product installer available**. Incomplete preview downloads have been withdrawn. This repository contains development source; successful client compilation is not a complete deployment.

The next distribution must include the matching Android/Linux system images, emulator and prebuilt runtime components. Users should configure device resources and a model in the client, then start using their devices. They should not have to find images, install an SDK, compile services or manually connect to a separately deployed backend.

| Target | Required distribution | Current state |
|---|---|---|
| macOS Apple Silicon | Complete local device bundle and client | Integrated runtime/bundle acceptance in progress |
| Windows x64 | Complete local device bundle and client | Native phone/backend integration unfinished |
| Linux x64 | Complete local device bundle and client | Managed runtime under development; complete bundle unfinished |
| Android ARM64 | Root/KVM host and matching local device bundle | Privileged runtime/image integration unfinished |

[Release requirements](docs/public/preview-release.md) · [Current status](docs/public/release-status.md) · [Issues](https://github.com/Conferringo/AI-Phone/issues/new/choose)

Source setup commands in the development guides are for contributors. They are not the intended product installation experience. Device disks, accounts, conversations, attachments and secrets never belong in a distributable image.

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

See the [compatibility matrix](docs/public/compatibility.md) for QQ, Feishu, WeCom, Telegram and local-QR WhatsApp scope. Adapter code does not establish end-to-end acceptance for every account. [Model and connection setup](docs/public/model-and-channel-setup.md) covers all 13 presets and the three API protocols. The desktop companion has been removed from the core; only a [future extension design](docs/public/companion-extension.md) remains.

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

---

# 简体中文

[Back to English](#ai-phone)

## 下载

目前**没有达到标准的产品安装包**，此前不完整的预览下载已撤下。仓库仍处于开发阶段，客户端编译成功不等于完整部署成功。

下一次发行包必须包含匹配平台的 Android/Linux 系统镜像、模拟器和预构建运行组件。用户在客户端选择设备资源、配置模型后即可使用，无需另找镜像、安装 SDK、编译服务或手动部署后端。Android 独立宿主优先支持 root 且开放 KVM 的设备。

[发行标准](docs/public/preview-release.md) · [当前进度](docs/public/release-status.md) · [Issue 反馈](https://github.com/Conferringo/AI-Phone/issues/new/choose)

开发文档中的源码启动命令仅供贡献者调试，不作为产品安装方案。设备盘、账号、聊天、附件、密钥与私有证据不会进入发行镜像。

## 技术说明

AI Phone 为 Agent 提供真实 Android 手机和轻量 Linux 桌面：查看设备画面、使用应用、处理文件，并在同一工作台连续沟通。默认一部手机、一台电脑，设备分别保存聊天，可共用 Agent 或分配不同模型。

操作依据真实观察，副作用绑定用户、设备、run、控制权 epoch 和 action_id；未知执行结果先观察，不盲目重放。模型仍可能误读界面，执行记录和独立核验用于检查结果，不能保证永不出错。

## 功能场景

- **手机应用协助**：操作微信等已安装应用，查看真实通知、整理消息，按用户指示回复。账号限制、登录和应用风控仍由应用自身决定；不承诺自动规避限制。
- **手机与电脑协作**：手机收到文件，经数据线传到 Linux，处理表格并核对结果，再传回手机。向联系人交付需有对应授权；文件传输成功与社交消息发送成功分开记录。
- **持续任务**：用户可连续补充、打断模型生成；消息持久化、旧正文与操作过程保留，工具未知结果不重放。
- **能力积累**：内置 phone-use、computer-use、文件传输及应用学习 Skill；知识库支持文档关联，记忆与外部连接归属 Agent。
- **客体网络**：Android/Linux 内各自安装 Clash 图形应用，用户在设备内管理网络；宿主默认直联。

QQ、飞书、企业微信、Telegram 及本地扫码 WhatsApp 适配范围见[兼容矩阵](docs/public/compatibility.md)；存在适配器不等于每个真实账号都已完成收发验收。桌宠已退出本体，只保留[独立扩展思路](docs/public/companion-extension.md)。

## 技术框架与目录

| 层 | 实现 | 目录 |
|---|---|---|
| 工作台 | Next.js App Router/Turbopack、React、Tailwind、shadcn/Radix | `apps/web` |
| 原生客户端 | Tauri 2；macOS 受管安装、本机/局域网连接 | `apps/client` |
| Agent与设备服务 | Rust、Axum、Tokio | `crates` |
| 协议与持久化 | protobuf、PostgreSQL/SQLx；Redis用于临时协调 | `packages/protocol`、`compose.yaml` |
| 运行与设备镜像 | 原生ARM64 Android Emulator、Docker Compose Linux | `infra`、`scripts` |
| 可复用能力 | 内置Skill、设备模板、版本与来源清单 | `skills`、`config` |
| 使用与技术文档 | 安装、架构、隐私、审计和发行范围 | `docs/public` |

[架构](docs/public/architecture.zh-CN.md) · [文档索引](docs/public/index.md) · [连续对话机制](docs/public/conversation-steering.md) · [数据与使用边界](docs/public/capabilities-and-data.md)

模型/API、渠道与记忆归属 Agent；Skill、知识库和工作文件共享。代码与用户状态分离，`.env`、设备盘、账号、聊天、附件和私有证据不进入仓库。更新采用版本化制品、备份、迁移和回滚，不能对用户运行目录直接 `git pull` 代替升级。

## 后续开发

1. 社区安装反馈、安装中断恢复、资源迁移和签名更新；未实测平台不标为通过。
2. 四平台输入、声音、触屏、语言及可访问性验收；Windows/Linux 宿主管理。
3. 继续收敛大型UI/执行模块的职责，补齐长时任务、宿主休眠/重启恢复。
4. 模型发现与能力检测、更多应用特化、Skill升级/回退。
5. 语音输入/播报 → 可打断实时语音 → 通话沙盒 → 用户配置的真实通信线路。

模拟器与eSIM界面不能提供运营商号码或蜂窝能力。电话方向尚未实现，具体阶段与验收条件见[路线](docs/public/release-roadmap.zh-CN.md)。当前产品门槛与本轮证据见[发行状态](docs/public/release-status.md)，不以源码上传代表正式产品验收完成。

## 免责声明与许可

本工具按现状提供。用户控制自己的设备、账号、模型服务和消息渠道，并对授权操作及使用结果负责。设备root仅属于客体，不代表任意宿主机权限。不要用于欺诈、身份冒用、未经授权的消息、数据访问或其他违法行为。

原创代码采用[Apache-2.0](LICENSE)。第三方依赖、SDK、客体应用、素材及商标保留各自权利；源码许可不覆盖它们的二进制再分发条件。完整说明：[免责声明](DISCLAIMER.md) · [隐私](PRIVACY.md) · [第三方声明](THIRD_PARTY_NOTICES.md) · [安全](SECURITY.md)。

开发与贡献见[CONTRIBUTING](CONTRIBUTING.md)。业务API使用protobuf，测试必须隔离，禁止上传用户状态或真实密钥。
