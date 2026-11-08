# 私有备份与恢复

[English](backup-restore.en.md) · 适用于 Apple Silicon macOS 源码安装。

Rust 命令 `assistant --state-backup` 负责备份与恢复。

备份包含账号、聊天、模型与渠道密钥、附件、知识、Skill、动作档案、Android 磁盘和 Linux 文件。备份使用哈希校验，请保存在私有加密存储中。

## 创建一致备份

在当前安装目录运行：

```sh
./ai-phone backup --output data/backups/my-backup.backup
```

工具核对服务/桌面属于本安装，停止本项目的任务、媒体、网页、Android和Linux，导出数据库与Linux根文件系统，保存设备文件后恢复原服务。其他项目与数据库不受操作。失败也会尝试恢复原服务；若恢复失败会明确报错。不要在备份期间手动重启设备。

推荐的`.backup`是权限0700的目录封套，内部文件权限0600，不会再复制一份完整磁盘来封包；可选`.tar`单文件需要额外临时空间。保留完整封套及SHA256旁文件；每个内部文件也有哈希及大小清单。临时socket、锁、日志、旧备份和安装下载缓存不复制；Android实际依赖的磁盘底层文件会保留，系统镜像依赖以SDK相对路径和哈希记录。数据库使用无owner/ACL的定制格式，恢复时重新应用运行/迁移权限。

## 恢复到空目标

先准备独立、空的PostgreSQL 17数据库，不能指向当前正在使用的数据库。匹配版本的Android SDK及system image必须已经安装。恢复目录也必须为空，允许仅有空的data子目录。

```sh
./ai-phone restore --archive /private/my-backup.backup \
  --destination /private/AIPhone-recovered \
  --pg-container my-empty-postgres --database ai_phone_recovered
```

目标PostgreSQL管理员需与备份`.env`中的管理员一致；命令经容器本地管理连接导入。现有表或用户文件会导致拒绝，不提供覆盖式恢复。导入表结构、内容和权限在一个事务中进行；导入后逐表比较行数及行摘要。文件逐一校验，稀疏磁盘恢复为空洞，Android绝对路径按新目录重定位，并验证SDK基础镜像。

恢复结果保持隔离：未完成任务转为暂停，自主探索及通知触发关闭，保留`data/restore-quarantine.json`，不启动Android、渠道或Agent。Linux根文件系统保存到`data/desktop/recovery`。可使用`./ai-phone recover-desktop --destination /private/AIPhone-recovered --desktop-container aiphone-recovery-check`导入并启动离线验证容器：不发布端口，不替换原容器，仅挂载恢复目录。导入前预估解压空间并保留4GiB空闲。`data/restore-report.json`记录校验结果。原实例仍能使用；不能让原副本和恢复副本同时联网使用同一登录账号。

启用恢复环境前，先停掉旧实例，再核对程序与用户目录的链接、Linux 导入结果、设备绑定和网络。核对完成后，由维护者解除隔离并启动。

## 验证范围

隔离恢复演练已核对数据库行和状态文件，启动恢复后的 Android 及其已安装应用，并在 Linux 桌面打开恢复的工作文件。
