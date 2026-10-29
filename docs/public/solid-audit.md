# SOLID / Clean Code Audit — 2026-10-28–29

本次完成全仓自动扫描、风险驱动代码审查与首批高可信修复（不是逐行形式化证明），**尚未达到全仓严格 SOLID / ub-mode 验收**。不能把编译通过、文件拆分或局部 100% 变异分数当作整个产品已成熟。

## 范围与方法

- 基线来自源码发行白名单的 587 个文件；按前端、BFF、助手、设备执行器、渠道、媒体、原生客户端、数据库/迁移、协议、部署与备份分域检查。主数据、账号、聊天、设备磁盘和密钥不进入扫描副本或公开证据。
- `crapper` 在基线识别 783 个函数，219 个 CC ≥10，39 个 CC ≥30。没有全仓覆盖率，因此全仓 CRAP 为 N/A，不能填零或宣称全部 <10。工具将部分 React 内部回调计入组件复杂度；非标准命名的测试模块也可能被统计，该数值是审查线索，不是生产函数或独立缺陷总数。复扫识别 803 个函数、219 个 CC ≥10、重复候选仍为 22 对：本批没有把全仓复杂度下降作为已完成成果。
- `dryer --threshold 0.8` 报 22 对候选。人工核对语义与调用链；`cancel`/`resume`、不同错误码工厂、Radix 包装组件等形似但契约不同，不为了降低数字强行合并。
- 本批先保存基线，在隔离数据库/临时传输替身/本地模型替身复现问题，再修复、运行行为回归。未调用真实模型、发送消息、改生产数据、清空设备或上传公网。
- 第三方 vendor、生成 protobuf、构建产物不做业务重构；历史验证脚本、未发行原型和平台实机验收不计入“全面通过”。原生客户端沿用上一批平台证据，本轮没有重新验证四平台 UI。

## 发现与处置

| ID | 优先级 / SOLID 边界 | 证据与影响 | 本批状态 |
|---|---|---|---|
| S01 | P1 / S、D：知识库写入归属 | `knowledge_save` 可绕过 `knowledge_link` 的双向、目标存在、自关联和邻接上限规则，导致单向或悬空关联 | 已修复。短事务锁统一两种写入，双向修改、版本、去重和回滚在同一事务内；不改 schema/protobuf |
| S02 | P1 / S：传输状态与事务 | 文件复制持有连接行锁时执行长 ADB；runtime 空闲事务期限比 ADB 期限短，锁可能丢失；目标已落盘但回包丢失曾被标 failed | 已修复。复制/校验在事务外，交付前重新核对线缆版本；先持久化 unknown，再做有界最终发布，确认提交才 completed，不盲重放 |
| S03 | P2 / S、I、D：媒体连接生命周期 | 两个 BFF 路由重复持有 reader、检查定时器和取消监听；已取消请求仍可打开上游，静默上游缺少显式主动清理 | 已修复。共享 `media-relay.ts`，路由保留各自授权/设备解析，单连接对象拥有所有释放路径；桌面设备解析有截止时间并返回 protobuf 错误 |
| S04 | P2 / L：模型取消契约 | 429 错误体读取未监听取消；服务端迟迟不结束响应时，停止可能等到 180 秒请求期限 | 已修复。错误体读取同样响应 CancellationToken；本地真实 HTTP 挂起响应先红后绿验证 |
| S05 | P2 / S、O：工具声明与执行混合 | worker 同时负责模型 schema、队列、策略、证据、执行、恢复 | 部分修复。声明移到 `tool_catalog.rs`，搬迁时函数逐字节相同；新增唯一工具名、参数完整性及设备意图契约测试。执行循环/分发仍需继续拆职责 |
| S06 | P2 / CI 边界 | 新持久化测试所需角色与原 CI 不一致；随机映射端口在容器重启后改变；socket-only 初始化就绪可产生假阳性 | 已修复。专用 fixture URL/角色，校验固定 loopback 端口和专用容器标签，TCP readiness，完整 CI 从空容器重跑通过 |
| S07 | P2 / S、D：前端职责集中 | 基线 `PhoneWorkspace` CC 288、RegistryWindow 154、FloatingWindow 104、PhoneStreamImage 67；组件同时管理交互、轮询、输入、媒体和持久化草稿 | 待整改。按设备会话、输入、媒体展示、窗口生命周期提取有行为测试的边界；保持现有布局/入口，不以换框架或增加通用基类代替修复 |
| S08 | P2 / 持久化单一真值 | 自定义 Skill 先替换文件再 SQL COMMIT；进程崩溃或 COMMIT 回包丢失可能导致文件与数据库版本不一致 | 2026-10-29 已修复：数据库为提交真值，资源可恢复；真实COMMIT成功应答丢失、资源写失败与启动修复回归通过。未声称已发生生产损坏 |
| S09 | P2 / 备份隔离 | 在已有其它安装的共享 PG 集群恢复时，固定全局角色密码可能被覆盖 | 2026-10-29 已修复：既有角色凭据必须兼容，恢复不改全局密码/权限；共享/空集群、PG原生SCRAM/MD5回归通过 |
| S10 | P3 / 查询边界 | 知识库 OR 搜索与组合表达式 GIN 不匹配，EXPLAIN 未使用文本 Index Cond | 待整改。保留原 OR 语义，评估独立索引或候选过滤+精确复核；未为小数据量贸然新增迁移 |
| S11 | P2 / 可读性与静态门槛 | 大量冗余嵌套、无效 Default 更新、冗余 clone/reference、迭代尾部扫描 | 机器可证明简化已应用并审查，未改协议行为。2026-10-29 四处HTTP错误边界与三个检查点/补充指令参数对象已处理；严格Clippy通过，没有添加allow掩盖告警 |

S01–S04 的复现、修复与回归可信度高。S07 是结构风险，S10 是有查询证据的未修复边界；S08/S09 已于下一批通过故障回归，不能用本次测试通过抵消。

## SOLID 的落地约束

| 原则 | 本项目的实际要求 |
|---|---|
| S：单一职责 | UI 展示、会话调度、模型运输、执行策略、设备 I/O 和持久化分别有明确 owner；一次媒体连接由一个 owner 释放。长函数先识别变化原因，不按行数盲拆 |
| O：开放封闭 | 新模型通过既有适配器返回统一结果；新设备扩展声明/执行能力，不能到处增加按名称判断。当前大工具分发仍是整改重点 |
| L：可替换 | 任一模型/设备实现都必须保持取消、错误、能力验证、未知结果不重放和作用域语义；HTTP 成功或返回字符串不代表动作完成 |
| I：接口隔离 | 媒体转发只接收连接/授权能力，不依赖整个注册表或页面；设备动作、观测、知识库写入、渠道送达保持各自契约 |
| D：依赖倒置 | 核心策略消费规范化证据和明确端口；传输、SQL、文件与平台运行时是边界实现。采用真实替身测试，而非为每个函数新增 interface |

术语：Device 是真实受控实例；Agent 是配置/记忆/连接归属；Model Profile 是不可变版本配置；Run 是持久任务；Action 是带唯一标识和控制 epoch 的一次副作用；Observation 是带版本的返回证据；Unknown 表示不能判断副作用是否发生，必须观察而非重放。

保留的有效设计：PostgreSQL `SKIP LOCKED` 持久队列；runtime/migrator 权限分离；不可变 Run/模型绑定；动作账本；私有用户数据与发行源码分离；Redis 作为可选缓存而非再造第二个任务真值。单管理员局域网产品不机械新增多租户 RLS。

## 技能使用记录

`ub-mode` 安装自 `unclebob/ub-mode`，固定提交 `57a8bdd28e45ed41900ace0bafa2ff76dd9ed70f`。本轮直接读取并执行其红绿、边界归属、重复代码、复杂度、变异测试与记录要求。其 push 建议受项目局域网/不上传约定约束，本轮没有 push。

| 技能 | 本次适用范围 |
|---|---|
| ub-mode、simplify-codebase | 全仓证据、候选取舍、小批修复、恢复副本、红绿与验证记录 |
| backend-patterns、postgres-patterns | BFF 生命周期、数据库权限/事务/并发/索引；PostgreSQL 技能要求的 database-reviewer 并行专项审计 |
| ui-ux-pro-max、hallmark | 前端结构/一致性静态审计；不把营销站点的字体/装饰规则强加到既有工作台，本轮未重新做实页视觉验收 |
| gsap-react、gsap-performance、animista | 检查现有 GSAP context、卸载/隐藏、减少动效与单一 transform owner；不引入新动效库 |
| gsap-core、gsap-timeline、gsap-plugins、gsap-utils、ui-interaction-kit | 既有布局动画的后续整改参考，当前没有改动画实现，未虚报完整实页通过 |
| gsap-frameworks、gsap-scrolltrigger | Vue/Svelte/滚动驱动场景不适用于本批，未执行不相干改造 |
| iso-glow、open-design、design-taste-frontend | 新视觉/品牌/模板任务未发生；沿用项目 DESIGN，不套用新主题 |
| namethatui、react-bits、magic-ui | 无需识别未知组件或安装新组件，未为“激活”增加依赖 |

技能不是需要全局开启的一套运行插件。已逐项判断适用边界，不把未适用技能冒称执行完成；后续修改具体窗口仍须按 AGENTS 的实页门槛验收。

## 验证

- `scripts/bun run check`：protobuf lint/build、4 个 TypeScript 检查、Rust workspace check 通过。
- Bun：88 项完整本地测试通过；媒体新增 11 项，覆盖取消、EOF、读失败、会话失效/抛错、慢检查、防止重入与错误响应。
- Rust：81 项非忽略单元测试通过；18 项显式依赖数据库/模型/子进程的测试保持 ignored，另行执行可隔离的部分。
- 完整隔离 CI：16 项 Rust 数据库集成、3 项权限、6 项备份恢复、5 项任务进程故障恢复通过，包含 56 轮和 SIGKILL。真实模型测试及子进程入口不混入普通测试。
- Python 普通测试：36 项中 31 通过、5 项要求显式隔离环境而跳过；其中数据库/恢复测试已在上述 CI 补跑。
- `scripts/bun run build`：生产前端构建通过。
- 媒体模块使用 Bun LCOV + crapper：5 个函数 CRAP 为 1–8；mutator：15 个可生成变异点全部被检出，0 存活。该工具包含编译失败型 killed，分数不是安全证明，也不外推全仓。
- 源码隐私/许可元数据扫描通过。本轮不把旧安装包重新标为包含新修复，不替代签名、第三方素材权利、干净主机安装和多平台视觉验收。

工具固定提交：crapper `6fd92c4c9bcc56efb64a2f8743d5a2993bcfd42d`；dryer `66ff6d21a42c04afcad89c78a80066176d1294b0`；mutator `a5a0cb803b66e0cdb8bf471b3b08af701d424a41`。工具与测试副本只在忽略的审计目录中，不加入产品依赖或发布包。

## 下一批验收顺序

2026-10-29新增客户端本机部署边界：Native仅负责本地IPC、资源校验与进程生命周期；`client_host_entry.py`负责解包与单进程锁；`client_host.py`负责macOS依赖/宿主适配；已有`InstallState`继续拥有安装检查点，业务设备创建复用现有protobuf接口。隔离的取消、路径完整性、许可证、外来容器拒绝与数据保留验证通过。全新双设备安装、跨平台supervisor、升级激活/回滚仍未通过，不能据此新增“全仓完全SOLID”或“四平台已可发行”的结论。

1. Skill 提交/物化恢复与共享集群恢复保护；先写故障回归，不动既有用户资料。
2. Run 的检查点/暂停/恢复状态收敛成领域对象，拆执行策略与工具执行；严格 Clippy 已关闭，继续领域边界拆分。每批复跑进程崩溃恢复。
3. PhoneWorkspace、RegistryWindow、FloatingWindow 分职责拆分；逐窗口验收输入、失败、焦点、窄屏、减少动效和设备不中断。
4. 真实多平台媒体长稳、安装/升级/备份恢复与许可门槛通过后才提升发行状态。

## 恢复与操作收据

源码原件、工具输出、红绿日志、变异副本与 diff 位于忽略的 `data/audit/solid-20261028/`。源码修复无需数据迁移；按 changed-files/diff 恢复对应文件即可，不能覆盖其它批次用户修改。测试只创建有标签的短生命周期 PG 容器和临时数据库，完成后删除。已有手机、电脑、账号、聊天不改。

本次退役的是两份媒体生命周期实现和执行器内的工具声明职责；新增一个媒体 owner 和纯工具目录，不新增产品依赖/业务端点/迁移。未退役项都有明确后续验证条件，不能把剩余复杂度计为完成。

## English summary

This repository-wide audit found and repaired inconsistent knowledge links, unsafe file-transfer transaction boundaries, duplicated media lifecycle ownership, and cancellation during stalled HTTP 429 responses. Tool declarations were separated from execution without changing their schema. Isolated database, backup/restore and crash-recovery suites passed. The media relay passed all 15 generated mutation tests and scored CRAP 1–8 with measured coverage. This is not a whole-repository SOLID certification: large UI/execution modules and broader architecture work remain. The 2026-10-29 follow-up closes Skill materialization recovery, shared-cluster restore protection and all seven strict Clippy findings; see [stability audit](release-stability-2026-10-29.md). No private user state was exported and no public repository was created.

## Preview.2 installer ownership increment — 2026-10-29

The native installer now separates checkpoint persistence, pinned download/cancellation and child-process supervision. Completed installation is tied to an exact payload and successful process exit; it never substitutes for a live service probe. Recovery uses the same owned source/user directories and does not force-restart live jobs. Changed functions have scoped native/process/HTTP and isolated Python tests; coverage/CRAP remains unavailable, so cyclomatic-complexity results do not certify full repository cleanliness. Fresh-host and whole-system lifecycle acceptance remain open.

## Location and setup increment — 2026-10-29

Location policy, distance interpolation, guest adapters, protobuf persistence and map lifecycle have separate owners. UI snapshots and drafts use one reducer: stale polls cannot replace saved state, pending saves reject poll merges, validation errors survive polling, and unchanged routes are not redrawn. Scoped complexity-only analysis reports changed location functions at CC 1–9 after splitting the map lifecycle and draft admission. Coverage and CRAP are unavailable; this is not repository-wide certification. No duplicate backend candidates met the 0.8 threshold. Six selected Bun checksum/ZIP-boundary mutations were killed; broader mutation coverage remains open.

Static fixes, route advance, pause and clearing were checked against the existing Android and Linux Chromium guests. Location updates preserve screen grounding and cursor state and use durable phone action receipts. Generic Linux application location, cross-platform fresh-host setup and long-running location acceptance remain unverified.
