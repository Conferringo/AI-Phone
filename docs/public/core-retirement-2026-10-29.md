# 本体收敛与安装语言审阅

2026-10-29 · [Language choices / 语言](languages.md) · [可选角色方向 / Optional companion](companion-extension.md)

## R1 桌宠退出本体

- **范围与权属**：用户明确退役桌宠。生产消费者包括工作台共享入口与渲染、独立窗口、拖动/吸附/对白/偏好、执行记录向角色发送的公开进展和打开记录事件、页面锚点、翻译及素材清单。
- **清理**：删除12个专属源/测试/素材文件、13个专属翻译键；删除角色挂载、页面锚点、事件发送和接收、CSS及素材许可清单条目。退役7项专属测试；保留聊天、执行时间线、设备动作与发送队列测试。
- **持久化审计**：Rust后端、渠道适配器和protobuf中没有此功能的服务/消息；迁移文件与真实 PostgreSQL public 表/列检查均无角色专属对象。没有虚构数据库迁移，也没有清空通用配置、Agent记忆或聊天。浏览器旧状态只有两个专属 localStorage 键，升级清理按确切键删除，重复执行及存储拒绝可恢复；用户消息和 outbox 不受影响。
- **结果与证据**：真实旧素材请求404，工作台200；新原生客户端共享入口只保留Skill、知识库、工作文件、配置中心，Android/Linux画面仍在。现有消息300条、管理员1个。新发行源码白名单不包含旧组件或角色素材。
- **刻意保留**：公开审查文档中保留带日期的历史记录；私有旧审阅包和源码退役备份不是当前发行物。角色扩展只写方向文档，不在本体预置插件入口、服务或接口。
- **恢复路径**：源文件退役前保存在忽略的本地审计备份，若产品决策反转需重新审查后恢复消费者及素材许可记录。数据库没有删除操作；本体不提供旧角色位置/偏好的恢复按钮。

## R2 安装、连接和配置语言

八种语言已覆盖安装配置提示。语言是 `configure` 与首次 `install` 的第一项，接受编号或代码，错误选择可重试；私有配置和数据库凭据保持独立。原生连接选择通过有界导航参数传给工作台，cookie保存后移除该参数，保留其他查询参数。普通客户端重启不重放旧语言选择，避免覆盖之后在工作台改的偏好。语言改变不改客体系统语言、账号、Agent记忆和消息正文。

验证：安装隔离测试4项，覆盖首项提示、编号对应、非法输入重试、既有语言、私有部署保存和八目录完整性。真实Next路由八个合法值均307、正确cookie及参数移除；非法值不写cookie。Tauri原生边界5项包含语言白名单、旧连接文件、同源导航、认证和真实HTTP探测。

新版macOS客户端实际连接隔离protobuf服务：原生英语选择进入英语首配；八语言切换保留名称/API输入；模型检测失败保留输入，修正保存后进入设备步骤。隔离服务不转发模型调用、设备动作或渠道消息。随后恢复原真实服务和中文。

## R3 自绘选择控件与统一高度

生产工作台的17处选择入口（含语言）改用同一个 `SelectField`，使用现有Radix Select依赖，不增加动画/表单库。表单数据仍由原组件负责；菜单负责呈现、焦点和选择。菜单使用自绘边框、玻璃背景、当前项和高亮，支持方向键、Home/End、文字查找和Esc。弹层带所属窗口标记；窗口切换关闭旧菜单，Esc先关菜单，下一次关窗口。

统一文本输入和选择触发器高度为40px，粗指针44px；范围滑杆、开关、复选框、图标按钮和多行编辑器不压成单行。安装器的原生连接页语言菜单也改为自绘列表，使用相同高度及明确焦点/错误反馈。

原生实页验证：服务商菜单展开、Esc仅关下拉、键盘选择OpenAI后同步URL/模型/协议、切回DeepSeek、保存推进；默认Agent切换“未分配”保留空值并阻止未保存前下一步；撤销、电脑模板禁用态和CPU选择；第二次Esc关闭配置窗口。安全截图使用空白/隔离配置，没有真实模型密钥。源码仍保留不发行的lab原型；本批不把原型选单当作生产入口。

## 检查与限制

- 打包实测发现Finder `.DS_Store`混入白名单并导致Git审阅树导出失败；已补系统元数据排除（包括AppleDouble伴随文件），回归先失败后通过。最终源码包逐项核对哈希和退役资源缺席，未通过删除私有文件来掩盖导出问题。
- 源目录Bun77项/1,616断言；7项发行、4项安装语言、3项源码导出、3项客户端制品、1项原生清单Python测试通过。TypeScript、Next生产构建、Tauri macOS App/DMG及严格原生Clippy通过。数据库/protobuf业务未修改。
- 新存储清理与语言纯逻辑局部覆盖100%，CRAP2–7；Dryer0.8无重复；安装语言11/11变异击杀。TS变异6/6指已有locale分支，新短函数工具未生成变异站点，不伪称其变异覆盖100%。自绘组件依靠原生实页验证，本轮未取得其自动化行覆盖；不能外推为全仓SOLID通过。
- 浏览器控制在Work设置关闭，本批用原生App验收；新的390px下拉定位、触屏、安全区、全屏幕阅读器和四平台键盘/声音矩阵未完整实测，继续作为UI发行门槛。部分后端诊断仍为原文。
- 下次交付目标为可供公开前审阅的技术预览候选：源码、双语文档、许可/部署/测试清单、客户端校验和与明确平台限制。当前没有远端仓库；按项目约定仅本地/局域网交付。
- 四道产品门槛仍开放：完整UI/平台验收、全新物理主机完整安装、剩余26项完整notice与素材/二进制再分发审查、宿主休眠/重启/长期运行与更新恢复。不能承诺下一轮直接交付完整四平台服务器发行版。

## English

The companion has been retired vertically from the core: UI, events, exclusive state, assets and release manifests. Backend/protobuf/database audits found no companion-owned service or table to drop. An exact-key upgrade cleanup removes its two legacy browser preferences without touching conversations, outboxes or Agent memories. Only a future extension design remains in documentation.

Installation now asks for the interface language first. Explicit native connection choices are bounded, handed to the workbench and persisted in a cookie; ordinary restarts preserve later workbench choices. Production selects share a custom Radix-based control with 40px fields and 44px coarse-pointer targets; the native connection language list is custom too. macOS native checks and isolated configuration flows passed. Full native-platform, narrow/touch/accessibility, clean-host, licensing and long-running acceptance remain open; no remote publication occurred.
