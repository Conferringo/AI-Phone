# Task checkpoints and recovery / 任务检查点与恢复

PostgreSQL checkpoints each task's goal, context, pending calls, action IDs, model revision, retry counters, step, control epoch and wake deadline. Closing the window leaves the task active. If the runtime restarts during an update or after an interruption, it queues the last saved step again. Pause and Resume keep the same task and its original wait deadline. Stop ends the task.

If a dispatched action loses its receipt, the task pauses for verification. Resume marks the outcome as unknown, withdraws the rest of that tool batch and reads the current device for an interrupted phone or computer action. The model continues from that evidence. A new instruction sent to the paused task uses the same recovery path.

## Model configuration on recovery

Model changes take effect for an existing task only when it is paused and the user explicitly resumes it or submits a new instruction to that paused user task. Recovery resolves the current model connection through the task’s original Agent identity. The existing task keeps its Agent instructions and device authorization. Finish or stop the task to switch profiles. Running and queued tasks retain their pinned model revision.

Recovery locks the conversation, then the paused run, then the configuration registry. It rechecks the conversation and Agent identity before selecting the saved revision. A replacement revision must have passed the configuration check. Selecting the revision, preserving notice provenance, resetting retry state and queueing the task commit together. A missing or unverified replacement leaves the checkpoint unchanged; a failed follow-up submission also rolls back its message and receipt.

Recovery preserves the raw context, pending calls, action IDs and execution records. Changing provider, protocol, endpoint or model establishes a trusted checkpoint boundary: outgoing requests exclude older provider-private reasoning and signatures while retaining tool results as historical evidence. New complete native tool exchanges remain available to the selected model. Credential-only recovery retains the established replay boundary. Unknown side effects still require outcome verification and are never blindly replayed. Operational failures retain their classification after recovery instead of reappearing as assistant dialogue.

## 恢复时的模型配置

已有任务只有在暂停后由用户明确恢复，或向该暂停的用户任务追加指令时，才会采用更新后的模型配置。恢复流程通过任务原有的 Agent 身份读取当前模型连接，保留原任务的 Agent 指令与设备授权。完成或停止任务后可切换配置。正在运行或排队的任务继续使用已固定的模型版本。

恢复依次锁定对话、暂停任务与配置注册表，重新核对对话及 Agent 身份后选择已保存版本。替换版本必须通过配置检查。模型选择、通知来源保留、重试状态重置与重新排队在同一事务提交。替换版本缺失或未验证时，检查点保持不变；追加指令失败时，对应消息与接收凭据也整体回滚。

恢复过程保留原始上下文、待执行工具、动作 ID 和执行记录。更换服务商、协议、端点或模型后，出站请求按可信检查点边界移除旧模型的私有推理和签名，同时将原工具结果保留为历史证据；新模型的完整原生工具往返仍可正常传递。仅修正密钥时保留已有边界。结果未知的副作用仍须核对，不直接重放。运行故障在恢复后仍归类为系统通知，不重新进入助手对话。

## Failure behavior

| Failure | Behavior |
| --- | --- |
| Worker killed/restarted | Reload persisted checkpoint; completed tool results are reused, not executed again |
| Persisted wait | Retain its original deadline; one wake event when due |
| PostgreSQL unavailable after claim | Hold the claimed run; retry only its idempotent result transaction after recovery |
| In-flight side effect has no result | Pause for outcome verification; never blindly run it again |
| Provider 503 / temporary 429 | Persist bounded backoff/retry count, including across restart |
| Explicit exhausted quota | Pause immediately; preserve task and records |
| Partial/incomplete model stream | Discard unfinished output; no speculative tool execution |
| Long foreground task | Continue in slices; preserve completed effects |

Runtime result finalization locks the run and updates the run, message and top-level event in one transaction. It preserves cancellation and pending user steering. If PostgreSQL is unavailable, the worker holds the claimed run and retries finalization. Unresolved tool records pause the run.

Host sleep/reboot, power-loss recovery and prolonged multi-device operation have separate platform qualification. Current availability is maintained in [release status](release-status.md).

## 任务中断与继续

关闭窗口后，任务可以在后台继续。服务因更新或意外退出而重启时，运行中的任务从最近已保存的步骤重新排队。对话底部的「暂停」保留同一任务和原等待截止时间，「继续」恢复执行，「停止」结束任务。

已派发的操作缺少回执时，任务会暂停。继续或向该任务追加指令后，执行器保留结果未知的记录，撤回同批尚未执行的操作。如果中断发生在手机或电脑上，先重新读取当前界面，再依据新的证据规划。

## 安装中断

安装检查点在私有 `data/deployment/install-state.json`。SDK许可、SDK包、依赖、Clash客户端、工作镜像、桌面镜像和服务分阶段保存状态。进程独占锁避免同一user目录并发安装；完成步骤只有源码/配置指纹和当前产物均匹配才会复用。Docker镜像ID改变、缓存损坏、产物缺失或输入变更会重新执行相应步骤。状态文件原子替换并同步目录；不记录密钥或命令输出。

强制结束安装后可以重跑 `./ai-phone install`；未完成阶段会重做，已验证阶段会复用。SDK许可仍交给官方交互提示，不自动接受。启动器先验证所有已加载服务属于本目录，再只补缺失项；已有助手、手机、对话不会因安装重试而重启。Android输入法/桌面/Clash也仅在缺失时补装。

`./ai-phone start`可恢复缺失服务；`stop`先核对默认服务的工作目录，拒绝停止其他checkout的实例。Linux容器、数据库卷和账号不由该命令删除。升级源码或数据库前执行备份和迁移流程。
