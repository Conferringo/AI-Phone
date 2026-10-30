# Task checkpoints and recovery / 任务检查点与恢复

2026-10-28 · verified on the current macOS ARM64 source preview.

The PostgreSQL run checkpoint stores the goal/context, pending calls, unique local action IDs, model revision, retry counters, step, control epoch and original wake deadline. The public execution ledger is separate from the bounded model-request projection. Closing the browser is not cancellation. Each foreground slice yields after eight model turns without marking the goal complete.

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
