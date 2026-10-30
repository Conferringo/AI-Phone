# 可选角色扩展方向 / Optional companion extension

2026-10-29 · 设计方向，尚未实现。桌宠已从 AI Phone 本体移除。本体不包含角色素材、开关、预留按钮、运行服务、数据库表或安装依赖。

## 为什么独立

设备使用、执行记录、消息与工作文件属于核心工作流。陪伴角色具有不同的渲染、交互、资源消耗与素材许可边界，用户应能独立安装、停用和卸载。移除角色不影响 Agent 的主动性、自主探索、公开执行进展或语音路线。

## 将来如何接入

- 首版可做独立客户端/浏览器扩展；只有用户主动安装并配对后才订阅授权设备的公开进展。不要为一个角色在核心预先引入插件框架。
- 通过版本化的只读事件协议接收 run/device/sequence、状态与已发布摘要；重连从游标恢复，缺失记录查询已有执行记录。完整性、权限检查和公开摘要由核心负责，扩展不读取模型密钥、私人推理或原始工具结果。
- 扩展拥有角色素材、动作、位置、语音和本地偏好；不新增核心数据库表，也不把角色状态塞进 Agent 记忆。卸载只删除扩展自己的状态。
- 扩展若提供“操作设备”能力，必须回到现有受授权的任务入口，保留用户/设备/run/epoch/action_id，不允许用角色点击绕过控制权和未知结果核验。
- 交互允许用户直接拖拽；不挡设备与输入区。窗口切换、减少动效、屏幕阅读器和触屏有明确退路。角色素材与声音分别记录来源和许可。

先验证只读事件订阅、乱序/断线恢复、卸载后不留监听与进程，再评估动作包或语音。旧角色图像、第三方参考和实现不随源码发行包预置。当前本体没有上述扩展 API；新增业务接口仍需先定义 protobuf、权限和验收合同。

## English

The desktop companion has been removed from the core. A future opt-in companion should be a separately installed client or browser extension with its own assets, lifecycle, preferences and licence records. Start with an explicitly authorized, versioned read-only subscription to published task progress; do not expose keys, private reasoning or raw tool output. Resume by event sequence, handle disconnects and remove listeners/processes on uninstall.

The core remains responsible for device control, action evidence and permissions. Any future control request must use the existing task authorization and action ledger. The extension owns its visual and audio state without adding core tables or Agent memories. No companion runtime, plugin API, asset pack or placeholder button is shipped today. Agent autonomy and the voice/telephony roadmap remain independent.
