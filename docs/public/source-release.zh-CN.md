# 源码构建与贡献

[English](source-release.en.md) · [源码仓库](https://github.com/Conferringo/AI-Phone)

源码包含 Next.js 工作台、Rust 执行服务、Android 桥接、Linux 桌面配方、protobuf 契约、内置 Skill、安装与备份恢复工具、测试及 CI。默认一手机一电脑，可分别绑定 Agent；PostgreSQL 保存任务检查点，Redis 为可选缓存。

原创代码采用 [Apache-2.0](../../LICENSE)，第三方依赖、素材与客体应用保留各自许可，见[第三方声明](../../THIRD_PARTY_NOTICES.md)。源码构建面向贡献者，完整用户安装包的内容与验收条件见[发行状态](release-status.md)。

## 环境与启动

本地已运行的源码宿主为 macOS Apple Silicon。需要 Node 22+、项目固定 Bun、Rust 1.96.0、Python 3.11+、Java 17、Android 命令行工具、Xcode 命令行工具与 Docker Desktop。完整新部署至少预留 35 GiB，默认手机存储为 16 GB。

```sh
./ai-phone doctor
./ai-phone install
```

SDK 对实际请求的包提示许可确认。宿主默认直联，客体各有 Clash 应用，订阅由使用者提供。在「配置中心 → Agent」配置模型及设备绑定。程序不提供开发者密钥或账号，重复启动仅补缺失服务并保留现有状态。

详见[安装](getting-started.zh-CN.md)、[预构建组件](runtime-bundles.md)、[架构](architecture.zh-CN.md)与[备份恢复](backup-restore.zh-CN.md)。当前验收范围不要求非 macOS 实机安装；未实测仍标为未验证，匹配平台编译和组件检查继续必需。

## 源码与用户数据

版本化代码与设备盘、账号、聊天、附件、密钥分离。导出工具按白名单执行隐私和许可检查，公开协作约定使用专门文档。本地环境文件、私人 Skill、实机截图及证据不能进入公开提交。

```sh
python3 scripts/check_source.py
python3 scripts/source_export.py --destination /path/to/new-review-directory --git
```

导出目标必须是新目录，产出源码清单和没有远端的本地审阅仓库。与公开 checkout 比较差异后提交，后续贡献保留公开历史。运行用户目录的升级使用版本化制品、备份、迁移和恢复流程，不直接用 Git 覆盖用户状态。

## 自动检查

Source CI 执行 protobuf、TypeScript、Rust、隔离前端与适配器、归档和安装契约、生产网页构建。预构建验证展开并执行实际包内文件。临时 PostgreSQL 检查迁移、受限角色、备份恢复与 worker 故障；重启测试按顺序运行，不使用真实设备、账号或付费模型。

Native CI 在匹配环境构建 macOS ARM64、Windows x64、Linux x64/ARM64、Android ARM64 客户端。通过表示编译及检查范围内的包内容通过，不代表完整双设备宿主验收。精确证据见[工程审阅](engineering/index.md)，使用范围见[兼容矩阵](compatibility.md)。
