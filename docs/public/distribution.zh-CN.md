# 客户端、运行环境、用户数据与升级

[English](distribution.en.md) · 更新：2026-10-30

安装包内置设备与服务。用户在客户端配置设备资源和模型。详见[下载与宿主要求](release-status.md)。

## 已实现的目录边界

原生首配使用Tauri应用数据目录下的独立 `host`：

```text
host/
  deployment.json             首配检查点
  tools/                      当前开发安装链的受管工具
  releases/<payload-sha256>/  校验后的版本代码和运行组件
    data -> host/user/data
    .env -> host/user/.env
  user/
    .env                      私有数据库环境
    data/                     客体、账号、模型、渠道与工作文件
```

`crates/host-runtime/src/source_cli.rs`负责按资源摘要暂存。重试核对已有文件、依赖链接与持久目录；代码被改或目标被占用时拒绝替换，不覆盖用户状态。[预编译网页组件](runtime-bundles.md)已包含Next standalone和固定Bun。

源码暂存命令复用原生的内容寻址暂存器。

PostgreSQL使用Docker命名卷。备份恢复覆盖数据库、Android状态、桌面根文件系统/主目录及私有文件。恢复到空目标，保持隔离直到明确验证/激活。内置Skill属于代码版本；用户Skill、使用经验、记忆、知识、附件与停用偏好必须跨升级保留。设备扩容和系统替换独立于应用更新。

## 客户端更新

客户端选择当前平台的正式发行，核验 Ed25519 签名和安装包摘要后安装。macOS 替换应用包并重新打开。Windows 运行现有 Inno 安装器并重新打开客户端，沿用已发布的 QEMU 底盘和原写入层。设备、模型与聊天使用独立的持久数据目录。

两端安装包验收后，在发行维护者本机签署更新清单。

```sh
./scripts/bun scripts/sign-client-update.ts /path/to/offline-key AI-Phone-0.1.7-macos-arm64.dmg AI-Phone-0.1.7-windows-x64-setup.exe AI-Phone-0.1.7-update.json
```

将两个安装包和已签名清单附到同一个 GitHub 正式发行。同一份清单上传为 `AI-Phone-update.json` 供工作台检查，同时保留带版本号的文件名供旧客户端读取。签名私钥由发行维护者离线保管。

完整包必须包含干净Android/Linux基础、模拟器、预编译服务与匹配运行环境，并附来源、声明和适用对应源码。详见[发行要求](distribution-requirements.md)、[当前状态](release-status.md)和[备份恢复](backup-restore.zh-CN.md)。
