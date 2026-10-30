# 发行交付标准与差距审计

> 历史设计基线（2026-10-28）。完整包要求以[发行要求](preview-release.md)为准，最新结果见[发行状态](release-status.md)和[路线](release-roadmap.zh-CN.md)。下文当时的平台、公开状态与声明缺口数字保留为审查背景，不是当前下载或支持清单。

[English](release-contract.en.md) · 2026-10-28 · 对照既有需求，不代表已完成发行。

## 结论

目标是用户下载客户端后，完成设备和Agent配置，就能使用自己的AI手机与电脑。公开源码、可读技术文档和可安装产品是三项独立交付，缺一项就不能称为完整产品发行。当前有本地源码预览包及macOS ARM64原生客户端测试包；尚无四平台完整部署安装器。原创代码继续采用Apache-2.0；第三方组件和素材按各自条款处理。

当前只准备本地文件，没有创建远端仓库或上传源码。技术预览可以先于完整产品发行，但必须写清平台、前置依赖和未完成项。进度以通过的验收项计，不使用没有测试依据的“95%”。

## 用户安装后的目标流程

1. 下载对应系统和CPU架构的安装包，打开应用，选择“本机部署”或“连接服务器”。不默认连接开发者主机。
2. 本机部署检测内存、磁盘、虚拟化和网络；下载经许可的组件、显示必要协议及系统授权，支持中断续装。普通用户不需要安装Rust、Node或手动编译。
3. 创建默认一部Android、一台Linux；选择已验证的系统模板、CPU、内存、磁盘和显示型号。型号档案不能冒充硬件认证；现有设备资源调整与重新安装必须分开。
4. 创建Agent时一次配置模型/API，再绑定设备、渠道和记忆。同一Agent可操作多个设备；聊天仍按设备分别保存。Skill、知识库和工作文件共享。
5. 进入实际设备画面，完成输入、声音、截图、真实操作与手机/电脑文件交换。各设备预装Clash图形应用，宿主默认直联。
6. 在应用内完成检查更新、备份、恢复与卸载；卸载程序保留用户数据，清除用户数据是单独明确的操作。

Android客户端首先连接用户自己的服务器。普通无root手机运行完整服务是另一个未验证目标，不把“有APK”写成“安卓可当服务器”。平台安装器可能需要操作系统授权或重启，不能承诺跳过这些步骤。

## 平台与安装包

以下是交付目标，不是当前下载列表。每项均须单独记录系统版本、CPU架构、硬件要求和实机结果。

| 平台 | 目标文件 | 本机服务器目标 | 当前证据与缺口 |
| --- | --- | --- | --- |
| macOS | 签名、公证DMG | 首先Apple Silicon；原生Android + Docker服务/Linux | ARM64源码与现有设备已验证，新增Tauri客户端.app/DMG；需已有部署，无Developer ID签名/公证，干净物理机完整部署待验。Intel未承诺。 |
| Windows | 签名NSIS安装器或MSI，首选一个主入口 | 首先x64；原生Android + WSL2/Docker | 尚无安装器、服务生命周期、路径/权限、OCR或虚拟化实机验收。ARM64另行评估。 |
| Linux | 首先AppImage及deb；rpm后续独立验收 | 首先x64；KVM Android + Docker + systemd | 源码检查使用Linux CI配置；不等于完整服务器已跑通。客户端、宿主OCR、设备发现和安装验收待做。ARM64另行验收。 |
| Android | 签名APK | 首版为远端客户端 | 尚无客户端APK、配对/TLS、触屏和后台恢复验收。已有输入法/桌面APK是客体辅助程序。 |

当前Android模板是Android 15 ARM64，Linux是Debian 13/Openbox ARM64。Fedora模板尚未可用；不同系统版本、CPU架构及现有设备扩容尚未形成完整产品流程。ARM64模板、第三方应用与原生桥接不能直接当成Windows/Linux x64兼容证据。

已采用Tauri 2实现原生客户端；独立的跨平台宿主管理层仍待实现。现有Next.js有服务端API路由，不能直接静态导出后宣称完整功能已封装：客户端应连接受管理的本机/用户服务器工作台，原生能力限制到明确命令及来源。远端网页不能获得宿主任意命令执行能力。[Tauri的Next.js集成要求静态导出](https://v2.tauri.app/start/frontend/nextjs/)，其[分发指南](https://v2.tauri.app/distribute/)也将各平台构建与签名单独处理。

## 公开仓库应该交付什么

| 内容 | 当前已有 | 发行还需要 |
| --- | --- | --- |
| 源码与目录 | Web、Rust服务、protobuf、渠道、媒体、设备桥接、基础设施、内置Skill；锁定依赖；精简导出白名单 | 可核对源码标签、二进制对应提交、可重复构建说明；实际远端CI运行 |
| 用户安装 | 源码安装工具、阶段重试；macOS原生客户端预览 | 受管理运行时、自动组件下载、资源/系统向导、其他平台客户端；干净机器安装与断点恢复 |
| 数据与升级 | 独立user目录暂存；数据库权限分离；一致备份、隔离恢复和客体恢复启动验证 | 签名版本清单、应用内更新、兼容检测、迁移与激活编排、失败回退和用户数据保留验收 |
| 真实能力 | 现有Android/Linux运行；原生定位/OCR；动作账本；连续任务；文件往返 | 每个平台真实手机/电脑闭环、长时间运行、宿主休眠/重启恢复；不能用模拟服务测试代替 |
| 模型/渠道/语言 | 两类API适配；已有渠道；八种界面语言目录 | 实际模型能力与渠道附件矩阵、各语言输入/错误/窄屏验收；不将目录数量当成完整国际化 |
| 文档与许可 | 中英README、安装、架构、备份、贡献、安全、Apache LICENSE/NOTICE、依赖清单 | 本标准、隐私/免责声明；二进制及镜像的逐组件许可、对应源码、签名与标准SBOM |

发行页应提供原生安装包、源码包、SHA256、签名与验证说明、逐平台最低要求、更新日志和已知问题。源码与安装包版本必须对应。源码包不含API、账号、聊天、设备系统盘、截图、附件、个人记忆或订阅。当前仓库中的`.github/workflows/source.yml`只是源码检查工作流，没有四平台安装器构建或签名作业，也没有远端运行记录。

## 目录边界

```text
apps/web                 Next.js工作台和服务端转发
apps/client              Tauri原生客户端与本地连接配置
apps/channels            外部消息渠道
apps/device-stream       设备画面、声音与媒体桥接
apps/device-network      客体Clash安装与完整性检查
apps/android-input       客体输入/通知桥接
apps/android-home        精简Android桌面
crates/assistant         Agent循环、工具、模型、权限与持久化
crates/phone-runner      root ADB、定位、OCR与动作执行
crates/api               Rust协议类型
packages/protocol/proto  业务protobuf源，先于端点变更
infra                    Linux/工作环境镜像配方
config                   系统预设、版本固定、许可与发行门槛
skills                   随版本分发的通用能力
scripts                  开发、安装、迁移、备份与源码导出
docs/public              可进入源码包的中英文文档
user                     用户持久数据，不进Git
dist                     生成的本地发行物，不进Git
```

`apps/client`已有原生客户端实现和macOS构建；跨平台宿主管理模块尚未实现。宿主管理需要逐平台替换launchd、Swift Vision OCR、Unix权限及设备资源管理等依赖。Redis目前为可选Compose服务，不是Agent任务队列；任务持久化不能以“有Redis镜像”代替验证。

## Apache-2.0的适用范围

推荐保持已选Apache-2.0，方便个人部署、商业集成和扩展。它包含贡献者一定范围的专利授权，但允许下游闭源修改，不强制回馈，也不授予项目或第三方商标。分发仍需保留许可/适用声明、标明修改；它不能替第三方组件授权，也不能证明项目不存在专利问题。[正式条款](https://www.apache.org/licenses/LICENSE-2.0)。

当前必须单独处理：

- GSAP采用自定义许可，有特定用途限制；不能称整套依赖均为宽松开源。[GSAP条款](https://gsap.com/community/standard-license/)。
- Clash/Mihomo及Linux镜像包含copyleft组件。分发二进制/镜像前需核对具体许可证、对应源码和声明；进程分离不自动解决衍生作品判断。[Apache与GPL兼容说明](https://www.apache.org/licenses/GPL-compatibility.html)。
- Compose固定Redis 8镜像，需明确适用的许可选项及镜像分发义务；不能沿用旧Redis的BSD假设。[Redis许可](https://redis.io/legal/licenses/)。
- SDK、模拟器、系统镜像、第三方APK及Docker Desktop不能统一改授Apache；优先合法在线获取，必要时由用户接受原协议。SDK开源组件与其他SDK包条款不同。[Android SDK条款](https://developer.android.com/studio/terms)、[Docker Desktop条款说明](https://docs.docker.com/subscription-billing/desktop-license/)。

完整清单见[第三方声明](../../THIRD_PARTY_NOTICES.md)。[免责声明](../../DISCLAIMER.md)和[隐私说明](../../PRIVACY.md)为工程草案，不替代许可或适用法律，也不通过附加使用限制改写Apache许可。

## 优先级与验收

1. **技术预览门槛**：完成素材/依赖分发审查、真实页面验收、干净Mac安装和宿主重启/长时间恢复。已通过的数据库权限、备份恢复和动作账本保留证据，不重新清空现有实例。
2. **首个可安装产品**：统一客户端与受限宿主管理；先交付macOS ARM64真实DMG，用户不编译；可创建默认设备、配置Agent、重试安装、保留数据卸载。先完成一个端到端安装器，再扩展各平台适配。
3. **多平台产品**：Linux服务器及客户端、Windows服务适配和安装器、Android远端客户端；各CPU架构单独通过支持矩阵，不虚报全平台完成。
4. **更新闭环**：签名、安装隔离、备份/迁移/健康检查/回退，打断安装和升级后用户数据仍在。通过后才接入更新按钮。

首次公开技术预览与四平台正式发行使用不同验收表。当前[发行门槛](release-status.md)仍未全部通过，尚未公开上传；本次审计不把待办改为完成。


2026-10-28 多平台补充：已生成macOS ARM64 DMG、Windows x64 NSIS、Linux ARM64 DEB、Android ARM64 APK。这些是连接已有部署的客户端，不改变上表对完整服务器安装器的门槛。实际运行范围及剩余门槛见[多平台验收](multiplatform-clients.md)。
