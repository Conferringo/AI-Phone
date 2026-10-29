# 私有备份与恢复

[English](backup-restore.en.md) · 适用于当前 macOS ARM64 源码预览。

备份包含账号、聊天、模型/渠道密钥、附件、知识、Skill、动作档案、两部手机的保留磁盘、Linux根文件系统和主目录。它是私有数据，不能放进公开发行包。当前格式使用哈希校验，不是加密或发布者签名；请保存在自己的加密磁盘或私有备份介质中。

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

当前恢复为维护命令，尚未实现UI一键接管、跨主机SDK自动下载、旧实例停机与新实例自动切换。启用恢复环境前需要核对旧实例已停机、程序与user目录链接、Linux镜像导入、设备绑定和网络，然后由维护者解除隔离并正常启动。不能仅删除隔离标记就把恢复校验当作完整部署验收。

## 验证范围

当前实例的完整备份已恢复验证：33张表、300条聊天、3019个状态文件；Android恢复盘真实启动并验证root/画面/微信与Clash保留，Linux根系统真实启动GUI并核对10个工作文件；两个副本均离线。隔离测试覆盖完整空库迁移、恢复前后逐表一致、运行角色检查、动作未知结果文件保留、稀疏文件/符号链接/硬链接、损坏包拒绝、路径越界拒绝、非空目标拒绝。当前环境整机演练结果持续写入开发进度；单独空白物理主机安装、断电和介质损坏恢复仍单列发行门槛。
