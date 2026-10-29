# 技术实现

[English](architecture.en.md)

| 组件 | 实现 | 职责 |
| --- | --- | --- |
| 工作台 | Next.js App Router/Turbopack、React、Tailwind、shadcn/ui、GSAP | 真实画面、设备透视切换、每设备对话、配置与共享资源 |
| 助手 | Rust/Axum/Tokio | 持久任务循环、模型适配、调度、记忆、知识、权限与完成审计 |
| 手机执行器 | Rust + ADB + Android桥接 | 当前观察、控件/OCR定位、输入/手势、epoch、动作去重 |
| 媒体 | Bun、Android gRPC、FFmpeg/X11 | JPEG帧流、PCM音频；原生触点/指针进入同一画面 |
| Linux电脑 | Docker、Debian/Openbox、Xvfb、桌面桥 | computer use、办公软件、客体sudo与文件处理 |
| 渠道 | Bun、官方SDK或Bot API | Agent级凭据、私聊配对、入站排队、送达状态 |
| 存储 | PostgreSQL/SQLx；私有数据文件 | 注册表、连续对话、模型版本与审计；Redis预留，尚未分发任务 |

浏览器业务请求使用 `application/x-protobuf`，契约在 `packages/protocol/proto`；媒体字节和健康检查例外。流式业务事件使用protobuf WebSocket帧，不要与模型SSE混淆。

## 一次真实操作

用户或已配对渠道 → 持久run → 固定Agent与模型版本 → Skill/当前观察 → 模型提出动作 → owner/设备/run/epoch/action_id校验 → 客体执行 → 动作后观察 → 工具结果落盘 → 下一轮 → 独立完成核验。

动作注入成功不代表目标完成；结果未知先观察，不自动重放。读取Skill不等于操作，通知预览不等于完整消息，输入草稿不等于已发送。界面保留公开进展、实际工具输入输出和审计，不伪造模型私有思维链。当前执行器串行，多Agent配置不等于并行执行。

Responses和Chat Completions复用同一执行引擎；后者转换函数调用、结果与图片，不改变持久证据。拒绝截断、过滤、缺少终止标志或无效工具参数的输出。新模型须实际通过能力探测，不能只检查HTTP连通。

## 设备与文件

默认一手机一电脑；root/sudo分别限定Android/Linux客体，不授予宿主任意执行权。数据线是持久连接关系，传输需连接及设备授权正确，核对方向、路径与哈希；断开后拒绝新传输。每台设备保留独立对话/草稿，即使共用Agent。

已验证测试文件的手机→电脑→手机处理链路；不等于微信实号交付。执行账本持久保留和隔离worker/数据库故障恢复已通过；宿主休眠/重启、长时间多设备运行及通知/任务竞争仍需发行验收。见[恢复验证](task-recovery.md)及[发行状态](release-status.md)。

程序使用版本目录；user保存私有状态，源码包不带用户数据。开发工作区仍保留原data目录，新managed staging使用受控链接接到独立user目录。见[发行与升级](distribution.zh-CN.md)。
