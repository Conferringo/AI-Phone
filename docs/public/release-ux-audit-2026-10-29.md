# 首次配置与窗口审查 / First-run and window audit

日期 / Date: 2026-10-29。范围：本机运行的工作台、真实 Android/Linux 只读画面和 DeepSeek 能力检测；输入、失败、保存等破坏性场景在同一生产前端的隔离 protobuf 服务中验证。隔离服务不转发任何设备动作、渠道消息或模型请求。不能将这些 UI 验收说成新主机安装或真实渠道收发验收。

## 本轮修复

- 首次配置复用唯一的配置中心，依次配置 Agent、设备、检查并进入；已配置实例不强制重走引导。进度由服务端已验证模型、默认 Agent 和设备授权关系推导，不用 localStorage 的“完成”标志冒充就绪。
- 完成步骤重新读取配置，再向绑定设备请求真实画面。Android 请求带控件观察，验证设备 ID、尺寸、图像和 observation_id。只有模型通过不代表设备已连接；离线时保留窗口和错误。
- DeepSeek 新表单采用官方示例的 Chat Completions + deepseek-flash；现存 Responses 配置不迁移、不覆盖。显式切换预设会同步 API 地址、协议和模型，并清空旧目标的凭据。OpenAI 预设与兼容服务仍要求填写确切模型 ID；不把任意模型宣称为支持视觉/工具。
- API 协议直接显示，代理地址归入“高级”。地址合法性、模型 ID、检测失败有具体反馈。保留用户要求的明文密钥查看/填写及显示开关。
- 步骤与分类在同一窗口内过渡，保留表单实例和草稿；减少动效时取消过渡。手机宽度下步骤不挤压成零碎字符。渠道与模型输入对齐；保存按钮左对齐。
- Linux 安装状态读取失败不再被静默吞掉。记忆和知识文档提交时锁定对应编辑输入，避免保存回执覆盖期间新输入。渠道保存时暂停轮询，避免旧状态回执覆盖保存结果。
- 原生客户端连接窗口统一为工作台的中性配色，补齐八种语言、键盘选择、错误反馈和减少动效；切换语言保留地址草稿。已重新构建并运行 macOS ARM64 App/DMG，未把其他平台旧包标记为本轮新版本。

## 逐步验收

“通过”仅针对下列场景；未执行项明确保留。

| 步骤 | 窗口/路径 | 健康状态 | 实际证据与范围 |
|---|---|---|---|
| 1 | 主界面与共享入口 | 部分通过 | 实页记录现有并排设备与独立聊天；切换全局窗口收起旧窗口。完整设备动画/所有尺寸组合未重新跑完。 |
| 2 | 首次配置 | 通过所列场景 | 隔离首次进入自动展开；错误 API 检测保留输入；修正后保存、转设备、离线阻止完成、恢复后进入。 |
| 3 | 模型配置 | 部分通过 | 实际 DeepSeek 流式文字、图片、工具往返全部通过；预设切换、密钥清空、可见协议、输入校验、撤销和草稿保留由隔离界面验收。其他服务商未用真实账号检测。 |
| 4 | 设备配置/安装 | 部分通过 | 实际手机与电脑画面返回；资源、默认 Agent、安装表单检查。默认主手机仍由部署 CLI 创建；本轮未重建现有设备。 |
| 5 | 连接应用 | 部分通过 | 隔离 Bot ID/Secret 填写，绑定失败提示，输入保留；未绑定真实账号发送消息。 |
| 6 | Agent 记忆 | 通过所列场景 | 多行输入、保存失败保留、修正重试、成功保存、切换后保留；未删除用户记忆。 |
| 7 | Skill 管理 | 部分通过 | 真实内置/自定义列表；隔离导入失败保留 URL；本轮未重复安装外部 Skill。 |
| 8 | 知识库/文档 | 部分通过 | 真实关系视图基线；隔离新建、多行输入、失败提示、切换收起、返回草稿、保存、关联入口和 Esc。关联写入/导入图片全组合未重跑。 |
| 9 | 工作文件 | 部分通过 | 真实文件列表、窄屏与功能切换；未向用户目录写入测试文件。 |
| 10 | 桌宠（已退役） | 不再适用 | 2026-10-29 用户决定从本体完整移除；当前范围见[退役与安装语言审查](core-retirement-2026-10-29.md)。原基线仅作历史证据。 |
| 11 | 自主探索 | 部分通过 | 实际手机状态/开关/间隔可读；保留原暂停任务，不因验收启动自主任务。 |
| 12 | 截图 | 部分通过 | 真实手机截图就地预览，保存/附到对话入口可见；本轮未发送截图给模型或远端。 |
| 13 | 数据线 | 部分通过 | 真实连接状态、文件选择及传输记录窗口可读；保留已有连接，本轮未再传输文件。 |
| 14 | 原生客户端/四平台 | 部分通过 | 新 macOS ARM64 App 实际运行；连接窗八语言逐项键盘切换、地址草稿保持、不可达服务错误及恢复连接通过。DMG 校验通过。Windows/Linux/Android 新连接窗、完整原生媒体/缩放/触屏矩阵未实测；安装包仍未签名/公证。 |

## 视口、语言和边界

- 桌面 1280×720，窄屏 390×844。八种语言的配置表单实际切换并测量：窗口 356px，scrollWidth 356px，无横向溢出。这不等于八种语言的全部窗口矩阵完成。
- 系统减少动效模拟下步骤按钮 computed transition 为 0s；步骤切换与 Esc 保持可用。原生移动键盘、安全区、屏幕阅读器、触屏拖拽仍待平台实测。
- 新增纯逻辑测试覆盖新安装无模型、未验证模型、错误绑定、可选未安装设备、错误目标身份、无图像/零尺寸、协议与 URL 边界。Crapper 最高 CRAP 9、行覆盖 100%；Dryer 0.8 无重复；43/43 变异被杀死。该结果仅属于 registry-setup 模块，不代表原有大型组件全部达标。
- 生产源目录 Bun 测试 81 项、1,624 条断言通过；前端 TypeScript 和 Next 生产构建通过。客户端包与原生清单 Python 测试 4 项通过；原生语言选择纯函数 CRAP 4、覆盖 100%、1/1 变异被杀死，仅代表这个小函数。统计只包含源目录测试，排除 dist、data 内历史拷贝。Rust/数据库/protobuf 未修改，本轮未将既有后端测试重标为新增验证。
- 私有截图、隔离测试脚本和检测回执保存在本地 audit 目录，不进入公开源码包。明文模型凭据未出现在公开证据中。

## English scope

The first-run flow now lives inside the single configuration center. It derives progress from persisted, verified model profiles and explicit device bindings, and performs a real frame check before finishing. Existing devices, profiles, accounts and conversations are preserved. Provider presets reset destination-specific credentials; the API protocol is visible. Form drafts survive errors and feature switches. Save-time editing and connection polling races were addressed.

The table distinguishes live reads, isolated UI writes and remaining acceptance work. DeepSeek text/vision/tool probes and both device observations passed live. Eight-language configuration checks at 390px passed. A rebuilt macOS ARM64 app also passed native connection-language switching, draft preservation, failure feedback and reconnection; its DMG checksum passed. Other platform binaries were not rebuilt in this batch. These results are not complete multilingual or native-platform acceptance. Clean-host installation, all-platform input/audio, source/asset licensing and long-running recovery remain release gates. See [release roadmap](release-roadmap.en.md).
