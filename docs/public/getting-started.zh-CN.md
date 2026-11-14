# 本地安装与配置

[English](getting-started.en.md) · 开发者源码安装

## 选择安装路径

下载前请先查看[发行状态](release-status.md)中的安装包供应情况和各平台验收范围。产品目标是安装客户端后选择语言、设备资源并配置模型，具体见[一体化宿主约定](integrated-hosts.md)。

下文介绍 macOS Apple Silicon 的源码开发环境。完整安装包见[下载](release-status.md)。Windows 后端开发见[WSL 指南](windows-backend-runtime.md)。

## 更新已安装客户端

有新版时，工作台右上角会显示更新图标。展开查看版本号、版本名和更新说明，选择**安装更新**。客户端会核验安装包、完成安装并重启，继续使用原设备、模型配置和聊天。Windows 安装时可能出现系统授权提示。

## 源码安装的支持范围

这条源码路径使用 macOS Apple Silicon；Android 15 ARM64 使用原生 Android Emulator，Linux 电脑使用 Docker 内的 Debian 13/Openbox。

源码安装需要Node.js 22+、Bun（项目自动安装）、Rust 1.95+（发行构建固定1.96.0）、Java 17、Docker Desktop、Xcode命令行工具，以及包含sdkmanager的Android SDK。可从各项目官方渠道安装。默认检测 `~/Library/Android/sdk`，兼容旧Homebrew路径；不要求相同用户名或固定局域网IP。全新镜像/编译预留35GiB磁盘，内存建议16GB以上。这些是当前源码开发依赖，完整发行客户端应随包供应所需组件。

## 首次安装

1. 在解压目录运行 `./ai-phone doctor`，处理失败项；它不会修改设备。
2. 运行 `./ai-phone install`，填写SDK/JDK路径、可选HTTP代理、新设备资源和语言。空代理代表直联；输入 `-` 清除已有值。宿主和容器地址不同，容器不能用宿主的 `127.0.0.1`。
3. 在SDK官方交互提示中确认许可；安装器不会自动接受。随后下载工具/系统镜像、安装项目依赖、构建工作环境与Linux镜像并启动。
4. 打开客户端或本地地址后直接进入工作台，不显示欢迎页。默认是单用户本地模式，保留已有账号凭据与会话；不显示账号/密码表单。首次进入后在配置中心填写Agent模型配置。
5. 「配置中心 → Agent」填写模型/API/Key，保存前完成文字、图片及工具往返检测。授权设备，并在设备中选择默认Agent。Key可明文查看或编辑。
6. 启动脚本创建手机。进入配置中心创建电脑。在手机内安装其他 Android 应用。

这是有前置依赖的源码引导安装。安装进度分阶段保存。解决报告的下载或构建错误后，再次运行 `./ai-phone install`；只有输入和产物校验匹配才复用已完成步骤。已有AVD、账号和数据库密码保留；重试启动只补缺失服务。当前没有跨阶段整机事务回滚，保留错误与服务日志。见[恢复与安装重试](task-recovery.md)。

## 资源与网络

`./ai-phone configure` 将本机预设保存在私有 `data/deployment/profile.json`（0600）。手机支持1/2/4核、1.5–4GB内存、16/24/32GB新盘；电脑创建界面单独指定CPU/内存。修改预设不会重建已有设备；资源迁移/磁盘扩容仍是发行阻塞项。Linux主目录由宿主目录承载，没有独立磁盘配额。

新部署默认直连。可在[工作台代理](network.md)导入并启用订阅。

## 日常操作

`./ai-phone start` 启动/打开工作台；`open` 只打开；`stop` 停止本项目服务，保留设备、数据库和文件。重复start不重启现有任务，只补缺失且属于本目录的服务；已加载但反复崩溃的服务仍需检查日志。登录自启与开机前无人值守服务不是同一种模式。

LAN地址见私有 `data/deployment.json`，只选私有网络接口。后端、ADB、数据库和媒体控制保持loopback/私网。通过受信 TLS 或自有 VPN 接入不可信网络。

## 打包与分离目录

将 `package` 输出的归档路径填入 `stage` 命令。

```sh
./ai-phone package
./ai-phone stage --archive /path/to/generated-source-bundle.tar.gz --destination /path/to/AIPhone
```

源码包附带文件清单和 SHA-256 校验值。暂存时校验文件，用户数据放在独立目录。首装创建 current 链接，后续版本暂存后等待启用。见[发行与更新](distribution.zh-CN.md)。

默认 `local_entry: true` 是同一可信局域网内的个人部署模式：能够访问网页的人可作为本机所有者进入。不要把这个入口作为多用户权限隔离；需要密码认证的既有部署可在私有配置中关闭本地入口，旧认证端点保留。

运行服务与数据库迁移使用独立角色；启动前显式执行迁移，运行服务不使用bootstrap超级用户。见[数据库权限](database-roles.md)。使用 `backup` 备份、`restore` 恢复到空目标，见[私有备份恢复](backup-restore.zh-CN.md)。

## Liberty（开发中运行时）

在共享对话中开启 Liberty，Agent 可跨设备选择活动、形成和调整目标。右上角的「任务与目标」可设置、修订或关闭明确目标，运行时会暂停 Liberty。Agent 可以在核验后保存可复用的方法。见 [Liberty](autonomous-exploration.md) 和[通知工作流](notification-workflows.md)。
