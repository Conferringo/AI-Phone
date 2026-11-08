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

## 升级约定

目标流程：签名版本发现 → 兼容/空间检查 → 任务检查点 → 一致备份 → 签名/摘要校验 → 暂存 → 明确迁移与健康检查 → 激活或回退。备份失败不进入激活；程序回退和数据库恢复分开，不可逆迁移需兼容的数据恢复。

当前提供本地暂存与恢复。签名版本发现和自动回退尚未实现。

完整包必须包含干净Android/Linux基础、模拟器、预编译服务与匹配运行环境，并附来源、声明和适用对应源码。详见[发行要求](distribution-requirements.md)、[当前状态](release-status.md)和[备份恢复](backup-restore.zh-CN.md)。
