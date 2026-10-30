# 本批代码审计 / Code audit

2026-10-28。本轮范围：设备内 Clash App 交付及旧核心迁移、本地进入接口、多语言窗口、布局动画和源码包；不代表整项目所有文件已经完成审查。

| 问题 | 修复 | 验证 |
| --- | --- | --- |
| 网页网络配置与用户控制方式不符 | 删除网页网络窗口/API；每台客体安装原生 Clash，旧核心交接后不再自动恢复 | 实际 GUI、原生导入/刷新、两端 TUN 与六项 HTTPS/哈希下载 |
| 两套核心争路由、Verge 新版本依赖 systemd | Linux 使用独立 Clash Party；停止并移除旧 Verge 与核心 | 进程/接口核对、真实容器重启 |
| 重启丢失 /dev/net/tun | 每次客体启动创建临时节点，客户端自行控制 TUN；遵守原生自启设置 | 发现失败后修复并再次重启，网络检查通过 |
| 新设备只提供空入口 | 官方 APK/deb 大小/SHA256 固定，创建流程幂等安装；Linux 镜像内含 GUI 应用 | 空白隔离 Linux GUI/程序/桌面入口；Android 重复安装不清数据；篡改/缓存回归 |
| 设备“+”与数据线联排重复 | 删除手动联排按钮，只由实际连接状态触发联排 | 实页连接、断开、双对话与真实线条 |
| 设备与文字共用缩放、方向未变的轮询仍会截断动画 | 外层布局 Flip、内层透视、聊天平移/渐显分离；只在真实方向改变时捕获；统一清理与反向接续 | 实页旋转、联排、快速反向、窄屏与减少动效 |
| 本地安装仍要求账号表单 | 单按钮 Enter，保留既有所有者凭据和会话；Origin、protobuf、限流与服务端令牌检查仍有效 | 独立 PostgreSQL 初始化/密码hash不变/旧会话仍有效/认证限流；实页收拢进入 |
| 翻译只覆盖导航、固定宽度挤压英文 | 统一八语目录和变量插值；补共享窗口/无效输入反馈；桌宠分段按钮按文字自适应 | 键集回归、英文窗口与390px实际页面 |
| 暂停任务阻止新通知 | 旧用户任务保持暂停，新通知单独只读摘要，不擅自恢复原任务 | 隔离 PostgreSQL 并发测试与真实外部随机通知→Agent摘要 |
| 2000次动作后硬停止、长期去重记录会到上限 | 分片持久记录、128条缓存、进程独占锁、旧格式备份与降级格式阻断 | 25项runner测试含2101条迁移；原1701条全保持；真实动作跨重启冷去重 |
| 旧LaunchAgent未加载ADB路径、冒烟测试仍指向P0 | 启动脚本真正重载服务环境；测试真实认证Rust protobuf业务接口 | 实际重启/手机HOME与冷去重，非法wire/媒体类型/匿名拒绝 |
| 外键与消息查找缺索引 | 0026仅添加15项索引，连接池设超时 | 当前24项迁移一致，无未索引外键、坏索引或空闲事务 |
| 源码包混入用户文件/凭据 | 白名单与明显密钥扫描，独立 app/user 暂存，校验清单与哈希，公开门槛拒绝 | 七项隔离安装/打包测试、实际内部包和已知私有值比对 |

原独立后台核心的认证/版本冲突/幂等回归仍作为迁移证据保留；不把退役网页网络界面的通过项当作现在的产品入口。原订阅、系统盘、账号与聊天均保留，截图证据不进入公开源码包。

## 仍待发行验收

干净机器从源码包安装与备份恢复；所有语言的后端错误/辅助技术/短窗口完整矩阵；数据库运行角色与初始化超级用户分离；宿主重启、休眠、模型故障与长任务恢复；整项目/EPIC素材/二进制许可；四平台签名客户端和正式更新回滚。源码安装器有前置依赖，不能宣传为四平台免依赖一键安装。

The reviewed scope is this productization batch, not a repository-wide certification. Native guest apps, local entry, shared-window localization, layout transitions and source packaging have concrete checks. Clean-host recovery, complete platform/language coverage, a non-superuser database runtime role, long-running reliability and redistribution licensing remain explicit public-release gates.
