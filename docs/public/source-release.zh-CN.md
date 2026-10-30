# 源码技术预览

2026-10-29 更新：用户授权公开实验性源码预览，不再等待独立设备安装验收；此项为维护者豁免，不记为实测通过。仅上传新导出白名单源码/文档，不上传本机历史、状态、安装包或私有证据。完整产品门槛与二进制许可仍独立。

[English](source-release.en.md) · 0.1.0-preview.1 · 2026-10-28

本批交付公开实验性源码预览与本地可审查源码树。原创代码采用[Apache-2.0](../../LICENSE)；依赖、参考素材和客体应用按各自许可证，见[第三方来源](../../THIRD_PARTY_NOTICES.md)。技术预览与稳定产品的门槛分别说明，未完成项不会写成已支持。

## 包里有什么

Next.js工作台、Rust执行引擎、Android桥接、Linux桌面配方、protobuf契约、内置Skill、安装/备份/恢复命令、测试、GitHub工作流和中英技术文档。默认一手机一电脑，可独立绑定Agent。Redis是可选缓存声明，实际任务检查点使用PostgreSQL。

不包含开发者的Git历史、绝对工作路径、节点、API、账号、聊天、设备磁盘、附件、私人Skill或实机截图。源码包不会复制整个开发目录；只从白名单取文件。一次性清理/改写聊天、历史验收脚本、已退役验证APK和旧的模拟设备原型页不进入包；真实设备控制、输入/通知桥接、知识库和主工作台完整保留。

## 从包启动

服务器当前验证平台为macOS Apple Silicon；先安装Node 22+、Bun（项目自动安装）、Rust 1.96.0、Python 3.11+、Java 17、Android命令行工具、Xcode命令行工具和Docker Desktop。默认新手机16GB，完整首次安装预留35GiB磁盘。

```sh
./ai-phone doctor
./ai-phone install
```

SDK许可由官方交互提示确认。宿主默认直联，客体各有Clash App；节点由使用者自行导入。进入后在「配置中心 → Agent」填写模型连接、API Key和设备绑定。没有默认开发者密钥/账号。已有实例的重复启动只补缺失服务。

这是有前置依赖的源码安装预览。独立源码依赖/构建、空库初始化和本机完整备份恢复已测；另一台全新物理机器、首次Agent配置与完整收交件仍待验收。详见[安装](getting-started.zh-CN.md)、[架构](architecture.zh-CN.md)和[恢复](backup-restore.zh-CN.md)。

## 本地审查仓库

```sh
python3 scripts/check_source.py
python3 scripts/source_export.py --destination /private/ai-phone-review --git
```

导出工具只向一个新目录写文件，拒绝覆盖。它生成文件哈希清单与独立Git仓库，初次只有一个本地提交、没有remote，不继承开发目录历史；公共协作约定从专用文档生成，不复制本机私有AGENTS.md。`--draft`明确加入本地审查标记；这不是发布授权。正式产品的`package --public`仍受未完成门槛限制。

## 自动检查

`.github/workflows/source.yml`固定checkout/setup-node的提交，仓库权限只读；不自动上传包、发布Release或调用真实模型。执行Buf/TypeScript/Rust、前端与适配器测试、安装/归档测试、生产构建、隔离数据库与worker故障演练。数据库重启演练与其他集成测试按顺序运行。CI在Linux构建源码不等于支持Linux整套服务器。

本地同等检查有独立证据；远端CI以仓库Actions实际结果为准，工作流已准备不等于通过。仓库源码预览不自动创建Release，完整安装包单独审查。

## 仍待验证

- 已移除森林/欢迎入口，直接启动工作台经真实浏览器验证；新增macOS原生客户端.app/DMG测试包。知识网络完整实页视觉、其他平台及全新宿主安装仍待验收。
- 干净物理机器完整安装、宿主休眠/重启/断电与长时间多设备运行。
- 全语言/辅助技术UI矩阵；真实QQ/企业微信/飞书/Telegram账号矩阵。
- 四平台签名客户端、完整自动更新/恢复激活、已有设备资源迁移。
- 二进制/镜像再分发的依赖通知、GPL对应源码与素材来源最终复核。GSAP等单独许可证不会被项目Apache许可证覆盖。

后续按「macOS源码预览 → Linux服务器适配 → 更新器与统一客户端 → 四平台签名验收」推进；不承诺当前普通Android手机能托管整套服务器。

## 在线源码与后续提交

仓库：[Conferringo/AI-Phone](https://github.com/Conferringo/AI-Phone)。后续以该仓库的正常checkout保留公开提交历史：先生成新的白名单导出，和checkout对比，再提交已审阅变化。不要将整个运行目录直接git add，也不要用每次新导出的初始提交覆盖已有远端历史。安装包与用户状态保持分离。
