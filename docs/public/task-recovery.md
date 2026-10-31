# Task checkpoints and recovery / 任务检查点与恢复

The PostgreSQL run checkpoint stores the goal/context, pending calls, unique local action IDs, model revision, retry counters, step, control epoch and original wake deadline. The public execution ledger is separate from the bounded model-request projection. Closing the browser is not cancellation. Each foreground slice yields after eight model turns without marking the goal complete.

## Model configuration on recovery

Model changes take effect for an existing task only when it is paused and the user explicitly resumes it or submits a new instruction to that paused user task. Recovery resolves the current model connection through the task’s original Agent identity. Its Agent instructions and device authorization snapshot remain unchanged; a different default Agent on the device cannot take over the task. Running and queued tasks retain their pinned model revision.

Recovery locks the conversation, then the paused run, then the configuration registry. It rechecks the conversation and Agent identity before selecting the saved revision. A replacement revision must have passed the configuration check. Selecting the revision, preserving notice provenance, resetting retry state and queueing the task commit together. A missing or unverified replacement leaves the checkpoint unchanged; a failed follow-up submission also rolls back its message and receipt.

Recovery preserves the raw context, pending calls, action IDs and execution records. Changing provider, protocol, endpoint or model establishes a trusted checkpoint boundary: outgoing requests exclude older provider-private reasoning and signatures while retaining tool results as historical evidence. New complete native tool exchanges remain available to the selected model. Credential-only recovery retains the established replay boundary. Unknown side effects still require outcome verification and are never blindly replayed. Operational failures retain their classification after recovery instead of reappearing as assistant dialogue.

## 恢复时的模型配置

已有任务只有在暂停后由用户明确恢复，或向该暂停的用户任务追加指令时，才会采用更新后的模型配置。恢复流程通过任务原有的 Agent 身份读取当前模型连接，保留原任务的 Agent 指令与设备授权快照；设备默认 Agent 的变化不会改变任务归属。正在运行或排队的任务继续使用已固定的模型版本。

恢复依次锁定对话、暂停任务与配置注册表，重新核对对话及 Agent 身份后选择已保存版本。替换版本必须通过配置检查。模型选择、通知来源保留、重试状态重置与重新排队在同一事务提交。替换版本缺失或未验证时，检查点保持不变；追加指令失败时，对应消息与接收凭据也整体回滚。

恢复过程保留原始上下文、待执行工具、动作 ID 和执行记录。更换服务商、协议、端点或模型后，出站请求按可信检查点边界移除旧模型的私有推理和签名，同时将原工具结果保留为历史证据；新模型的完整原生工具往返仍可正常传递。仅修正密钥时保留已有边界。结果未知的副作用仍须核对，不直接重放。运行故障在恢复后仍归类为系统通知，不重新进入助手对话。

## Failure behavior

| Failure | Behavior | Verified |
| --- | --- | --- |
| Worker killed/restarted | Reload persisted checkpoint; completed tool results are reused, not executed again | Actual SIGKILL/restart |
| Persisted wait | Retain its original deadline; one wake event when due | Worker kill, PostgreSQL restart, SIGSTOP/SIGCONT |
| PostgreSQL unavailable after claim | Hold the claimed run; retry only its idempotent result transaction after recovery | Actual container stop/start while model response completes |
| In-flight side effect has no result | Pause for outcome verification; never blindly run it again | Interrupted write fixture, no new write/model request |
| Provider 503 / temporary 429 | Persist bounded backoff/retry count, including across restart | Loopback HTTP provider + SIGKILL |
| Explicit exhausted quota | Pause immediately; preserve task and records | HTTP 429 with insufficient_quota; one request |
| Partial/incomplete model stream | Discard unfinished output; no speculative tool execution | Actual truncated SSE HTTP response |
| Long foreground task | Continue in slices; preserve completed effects | 56 real model/tool turns, unique memory records, SIGKILL/restart |

Runtime result finalization locks the run and updates the run, message and top-level event in one transaction. It preserves cancellation and pending user steering. The worker does not abandon a claimed job when PostgreSQL is down and does not re-run its tools while retrying this transaction. Existing unresolved tool records force an explicit paused state rather than an unsupported success/failure claim.

These are recovery tests with an isolated database and local deterministic model, not evidence of arbitrary model quality. No production phone/computer, real API quota, contacts or channels are used. SIGSTOP/SIGCONT is process suspension, not physical host sleep. Real host sleep/reboot, power loss and a prolonged multi-device soak remain unverified publication gates.

## Reproduce the process test

Build the assistant normally, then compile its tests with `cargo test -p ai-phone-assistant --no-run --locked --message-format=json`. Find the test executable in the compiler-artifact output. Prepare a dedicated PostgreSQL 17 container, named with `aiphone-`, labelled `dev.aiphone.test=permissions`, with fixture user/database `ai_phone`, fixture-only password `isolated-fixture-password`, and a loopback port other than production 55482.

```sh
python3 scripts/verify_task_recovery.py \
  --pg-container aiphone-recovery-test-pg --pg-port 55500 \
  --test-binary /absolute/path/to/test-executable \
  --output /private/new-recovery-check
```

The driver deliberately stops/restarts that disposable PostgreSQL container. Do not run unrelated integration suites on it concurrently. It creates and drops independent test databases; inspect the report, then remove the explicitly owned test container and its anonymous volume. It never invokes the production startup scripts.

## 安装中断

安装检查点在私有 `data/deployment/install-state.json`。SDK许可、SDK包、依赖、Clash客户端、工作镜像、桌面镜像和服务分阶段保存状态。进程独占锁避免同一user目录并发安装；完成步骤只有源码/配置指纹和当前产物均匹配才会复用。Docker镜像ID改变、缓存损坏、产物缺失或输入变更会重新执行相应步骤。状态文件原子替换并同步目录；不记录密钥或命令输出。

强制结束安装后可以重跑 `./ai-phone install`；未完成阶段会重做，已验证阶段会复用。SDK许可仍交给官方交互提示，不自动接受。启动器先验证所有已加载服务属于本目录，再只补缺失项；已有助手、手机、对话不会因安装重试而重启。Android输入法/桌面/Clash也仅在缺失时补装。

`./ai-phone start`可恢复缺失服务；`stop`先核对默认服务的工作目录，拒绝停止其他checkout的实例。Linux容器、数据库卷和账号不由该命令删除。源代码或数据库升级仍须按备份/迁移流程处理，部分安装恢复不等于自动更新器。已实现的恢复隔离机制仍有效，不能用启动重试绕过隔离。

9项安装/服务用例覆盖进程强杀、锁释放、并发拒绝、失败状态、源码变化、产物身份变化、仅补缺失服务、跨目录拒绝和恢复隔离。当前服务实测：全量重复启动未改变PID；移除LAN代理后仅恢复该代理，其余六个进程PID不变。
