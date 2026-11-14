# Data and privacy / 数据与隐私

## English

- **Local storage.** The instance stores accounts, conversations, device disks, application sessions, knowledge, memory, attachments, action records and credentials in its database, private directories and guests.
- **External services.** Model calls send task inputs to the configured provider. Inputs depend on the task and can include text, screenshots, files and tool results. Connected channels send messages and supported attachments to their platforms. Guest apps, browsers and proxy subscriptions contact their own services.
- **Access.** The administrator can view API keys. Some credentials are stored in restricted files or environment settings. Host, backup and guest administrators have access to the data under their control. Use passwordless personal mode on a trusted network.
- **Retention.** Clearing the visible conversation leaves stored history intact. Logs, backups, guest caches and provider copies have separate lifecycles.
- **Backups.** Include the database volumes, devices and private files. Protect backups as you would credentials and account sessions. Remove private data before sharing screenshots, logs or issue reports. [Backup and recovery](docs/public/backup-restore.en.md)

The project has no diagnostic upload service. Third-party instance operators should publish their access, retention and deletion policies and a contact address.

## 简体中文

- **本地存储。** 账号、聊天、设备盘、应用登录态、知识库、记忆、附件、执行记录和凭据保存在实例的数据库、私有目录或客体中。
- **外部服务。** 模型请求向所配置的提供方发送任务所需的文字、截图、文件或工具结果。渠道把消息及支持的附件发给对应平台。客体应用、浏览器和代理订阅各自连接外部服务。
- **访问。** 管理员可以查看 API Key。部分凭据保存在受权限限制的文件或环境配置中。主机、备份和客体管理员可访问各自管理的数据。个人免密码模式仅用于可信网络。
- **保留。** 清空对话视图后，原聊天仍保存在数据库。日志、备份、客体缓存和提供方副本各有保存周期。
- **备份。** 备份包含数据库卷、设备和私有文件，应按凭据和登录态的敏感程度保管。分享截图、日志或提交 Issue 前移除私人数据。[备份恢复](docs/public/backup-restore.zh-CN.md)

本项目未提供诊断数据上传服务。第三方实例运营者应公布联系方式和数据访问、保留及删除规则。
