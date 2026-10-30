# 数据与隐私说明 / Data and privacy

2026-10-28 · 自部署技术预览的工程说明 / Engineering description of the self-hosted preview

## 中文

**谁保存数据。** 服务运行在用户配置的主机。账号、聊天、设备系统盘、应用登录态、知识库、Agent记忆、附件、截图、操作记录、渠道凭据和模型配置保存在该实例的数据库/私有目录或客体内。受管理部署将程序与`user`分开；现有开发实例仍沿用原路径。PostgreSQL命名卷不在`user`目录内，单独复制该目录不是完整备份。

**哪些数据会离开主机。** 调用已配置的模型时，相关对话、截图片段、文件内容、记忆或工具结果可能发送给该模型提供方；具体取决于任务和适配器。渠道连接向对应平台发送消息及支持的附件；客体应用、浏览、代理订阅、包管理和更新/组件下载也会连接其服务。本地部署不代表全离线，删除本地数据不等于删除提供方副本；提供方的保存、训练或跨境处理规则需分别核对。

**密钥和访问。** 管理员按产品设计可以明文查看API Key。服务端凭据并非普遍加密保存，部分由受限权限的文件或环境配置承载；这不构成加密保险库的承诺。具有实例管理、主机、备份或客体root权限的人可能接触相关数据。不要将个人模式的无密码入口直接暴露到不可信网络；它不代表已具备多租户隔离。

**保存、隐藏和删除。** 记录用于连续对话、任务恢复和执行核对。当前没有统一的保存期限或一键彻底清除所有关联副本的保证。“清空前端显示”只影响视图，原聊天仍保存；回收站、历史配置、备份、客体应用缓存和外部平台副本可能另存。用户可编辑/删除支持的记忆与知识条目，但不应据此推断关联日志或备份同步擦除。完整删除/导出生命周期仍需发行验收。

**备份与共享。** 使用[一致备份流程](docs/public/backup-restore.zh-CN.md)覆盖数据库、设备和私有文件。备份可能含账号登录态和密钥，当前不保证自动加密或自动过期。提交问题、截图、日志和源码前自行移除个人资料和凭据。源码导出白名单排除私有状态，但不应把它当成任意新文件都无需检查的保证。

项目不要求把用户聊天、记忆或设备上传给开发者；公开源码也不包含这些数据。不能据此承诺所有依赖、客体应用和提供方都没有遥测。当前尚无正式发行的自动更新或诊断上传服务；新增外发用途须更新本说明并在相应流程告知。

如由他人运营实例，运营方应另行提供其身份、联系方式、访问/保留/删除规则及适用权利处理方式。该项目级草案不冒充所有部署者的最终隐私政策。[安全反馈](SECURITY.md)不应附带秘密、私人聊天或完整备份。

## English

**Storage.** The service runs on the user's configured host. Accounts, conversations, guest disks, app sessions, knowledge, Agent memory, attachments, screenshots, action records, channel credentials and model settings reside in that instance's database, private directories or guests. Managed installations separate application versions from `user`; existing development installations retain their paths. PostgreSQL named volumes are outside `user`, so copying that directory is not a complete backup.

**Outbound data.** Configured model calls may send relevant conversation, screen regions, file content, memory or tool results, depending on the task/adapter. Channels send messages and supported attachments to their platforms. Guest apps, browsing, proxy subscriptions, package managers and component/update downloads contact their own services. Self-hosting is not fully offline operation. Local deletion does not erase provider copies; review each provider's retention, training and cross-border practices separately.

**Credentials and access.** Administrators can view API keys as a product feature. Credentials are not universally encrypted at rest; some use permission-restricted files/environment configuration. No encrypted-vault guarantee is made. Instance/host/backup administrators and guest root users may access relevant data. A passwordless personal entry must not be exposed directly to untrusted networks and is not multi-tenant isolation.

**Retention and deletion.** Records support continuous conversation, recovery and execution review. There is no universal retention period or guaranteed one-action erasure of all related copies. Clearing the visible conversation preserves stored history. Trash, configuration revisions, backups, guest caches and external platforms may retain separate copies. Editing/deleting supported memory or knowledge entries does not imply erasure of related logs/backups. Complete export/deletion lifecycle acceptance remains open.

**Backups and sharing.** Use the [consistent backup workflow](docs/public/backup-restore.en.md) for database, guest and private state. Backups may include sessions and secrets; automatic encryption/expiry is not guaranteed. Remove private data before sharing issues, images, logs or source. The allowlisted source exporter excludes private state, but new distributable files still require review.

The project does not require uploading conversations, memory or devices to its developers; source exports exclude them. This is not a claim that every dependency, guest app or provider has no telemetry. No production automatic-update or diagnostic-upload service is currently released. New outbound purposes require updated notices in the relevant flow.

Third-party instance operators must separately identify themselves, provide contact information and explain their access, retention, deletion and applicable-rights procedures. This project-level draft is not their final privacy policy. Do not attach secrets, private conversations or full backups to [security reports](SECURITY.md).
