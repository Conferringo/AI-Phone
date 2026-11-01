# 本地安装与配置

[English](getting-started.en.md) · 开发者源码安装

## 选择安装路径

下载前请先查看[发行状态](release-status.md)中的安装包供应情况和各平台验收范围。产品目标是安装客户端后选择语言、设备资源并配置模型，具体见[一体化宿主约定](integrated-hosts.md)。

下文是面向开发者的 macOS Apple Silicon 源码路径，需要准备列出的工具、下载依赖并编译服务，不等于完整发行包安装。Linux 开发者请阅读独立的[受管 Linux 安装指南](managed-linux.md)。

## 源码安装的支持范围

这条源码路径使用 macOS Apple Silicon；Android 15 ARM64 使用原生 Android Emulator，Linux 电脑使用 Docker 内的 Debian 13/Openbox。以下前置条件用于开发环境，不代表全新宿主安装已通过验收。

安装器要求已有Node.js 22+、Bun（项目自动安装）、Rust 1.88+、Python 3.11+、Java 17、Docker Desktop、Xcode命令行工具，以及包含sdkmanager的Android SDK。可从各项目官方渠道安装。默认检测 `~/Library/Android/sdk`，兼容旧Homebrew路径；不要求相同用户名或固定局域网IP。全新镜像/编译预留35GiB磁盘，内存建议16GB以上。

## 首次安装

1. 在解压目录运行 `./ai-phone doctor`，处理失败项；它不会修改设备。
2. 运行 `./ai-phone install`，填写SDK/JDK路径、可选HTTP代理、新设备资源和语言。空代理代表直联；输入 `-` 清除已有值。宿主和容器地址不同，容器不能用宿主的 `127.0.0.1`。
3. 在SDK官方交互提示中确认许可；安装器不会自动接受。随后下载工具/系统镜像、安装项目依赖、构建工作环境与Linux镜像并启动。
4. 打开客户端或本地地址后直接进入工作台，不显示欢迎页。默认是单用户本地模式，保留已有账号凭据与会话；不显示账号/密码表单。首次进入后在配置中心填写Agent模型配置。
5. 「配置中心 → Agent」填写模型/API/Key，保存前完成文字、图片及工具往返检测。授权设备，并在设备中选择默认Agent。Key可明文查看或编辑。
6. 手机主系统由脚本创建；电脑在配置中心添加并安装系统。第三方Android浏览器等APK自动分发尚待授权与固定版本流程，新装镜像不能宣称包含所有应用。

这是有前置依赖的源码引导安装。安装进度分阶段保存。解决报告的下载或构建错误后，再次运行 `./ai-phone install`；只有输入和产物校验匹配才复用已完成步骤。已有AVD、账号和数据库密码保留；重试启动只补缺失服务。当前没有跨阶段整机事务回滚，保留错误与服务日志。见[恢复与安装重试](task-recovery.md)。

## 资源与网络

`./ai-phone configure` 将本机预设保存在私有 `data/deployment/profile.json`（0600）。手机支持1/2/4核、1.5–4GB内存、16/24/32GB新盘；电脑创建界面单独指定CPU/内存。修改预设不会重建已有设备；资源迁移/磁盘扩容仍是发行阻塞项。Linux主目录由宿主目录承载，没有独立磁盘配额。

新部署默认直联。旧实例没有profile时沿用 `.env` 中原代理，避免升级突然断网。安装器会下载校验固定版本的Clash客户端；手机与电脑桌面均有Clash应用，在设备内导入订阅并启用VPN/TUN。网页配置中心不管理设备网络；默认不携带节点。见[实际网络配置与验收](network.md)。

## 日常操作

`./ai-phone start` 启动/打开工作台；`open` 只打开；`stop` 停止本项目服务，保留设备、数据库和文件。重复start不重启现有任务，只补缺失且属于本目录的服务；已加载但反复崩溃的服务仍需检查日志。登录自启与开机前无人值守服务不是同一种模式。

LAN地址见私有 `data/deployment.json`，只选私有网络接口。后端、ADB、数据库和媒体控制保持loopback/私网。局域网HTTP不等于加密传输；不可信网络应先部署受信TLS或自有VPN，本项目不创建公网隧道。

## 打包与分离目录

将 `package` 输出的归档路径填入 `stage` 命令。

```sh
./ai-phone package
./ai-phone stage --archive /path/to/generated-source-bundle.tar.gz --destination /path/to/AIPhone
```

打包采用源码白名单、排除私有数据、检查明显凭据，生成清单与SHA256；默认生成内部审阅包；发行门槛未满足时附带 `NOT-FOR-PUBLICATION.txt` 标记。stage验证内容后放入版本目录，第一次创建current入口；后续版本只暂存，不热切换。user目录跨版本复用。SHA256不是发布者签名。见[发行设计](distribution.zh-CN.md)。

默认 `local_entry: true` 是同一可信局域网内的个人部署模式：能够访问网页的人可作为本机所有者进入。不要把这个入口作为多用户权限隔离；需要密码认证的既有部署可在私有配置中关闭本地入口，旧认证端点保留。

运行服务与数据库迁移使用独立角色；启动前显式执行迁移，运行服务不使用bootstrap超级用户。见[数据库权限](database-roles.md)。使用 `backup` 备份、`restore` 恢复到空目标，见[私有备份恢复](backup-restore.zh-CN.md)。

## 无指向自主探索（开发中运行时）

为已配置设备开启探索后，Agent自行选择应用与活动，不要求指定方向、轮换App或每轮产出心得；回复与安装开关仍有效。旧暂停任务单独保留，停止探索不会删除历史。见[无指向探索](autonomous-exploration.md)与[通知工作流](notification-workflows.md)。本节描述开发中运行时，完整安装包可用性见[发行状态](release-status.md)。
