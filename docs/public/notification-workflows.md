# Notification-driven work

## Wake and ownership

The assistant service polls Android's notification/accessibility feed and the Linux guest's Dunst notification inbox. Notifications can wake a configured Agent with exploration disabled. Each event retains its source device, event ID, source key and original receipt. The device's configured Agent and model are pinned when its run is created.

Phone collection, phone supervision and phone admission run in one ordered pipeline. Desktop collection and admission run independently, so a stalled device does not hold the other device's durable notification work. Each pipeline admits only its own device kind on the existing three-second cadence. New events can enter the queue in the same pass. Outgoing summaries have their own owner; dropping the coordinator cancels all three futures.

New owner instructions take priority. Existing tasks yield at checkpoints to let notification work proceed. A paused task keeps its history and unknown-action receipts while a new notification receives a separate run. Manual takeover remains authoritative.

`pending` events are deduplicated by ID and newer versions of a source key are coalesced per device. A scheduled run owns `assigned` events; events delivered to an existing run become `attending`. Successful audited work seals those events. Interrupted attention becomes a summary in the task history.

## Replies and work

With autonomous replies enabled, the Agent decides how to handle ordinary messages and defaults to replying. It first inspects the actual application and conversation. Advertising, duplicates, self messages and status notices can be left unanswered with an actual reason. This also runs with exploration off. The latest owner instruction can narrow or stop that delegation.

A delegated work request can follow this path:

1. Inspect the source conversation and send one progress reply.
2. Connect already-authorized devices, download the real attachment and transfer it with a SHA-256 receipt.
3. Process it on Linux, keep the input and output in a task directory under `/home/ai/Files`, and verify the result through computer use.
4. Transfer the result back and select it in the original application using phone use.
5. Check the original app for its outgoing delivery status.

The built-in `wechat`, `phone-use`, `computer-use` and `device-transfer` skills guide the workflow. The Agent decides whether to reply and uses tool evidence to report the result. Incoming content is treated as message text, with access limited to the authorized account, files and recipient.

`device_list` discovers the owner's managed devices even before a cable exists. Its `can_control` and `can_handoff` fields distinguish the current task's GUI authority from work delegated to a device's own Agent. A task controlling either endpoint can connect the owner's phone and computer for file exchange. Each Agent keeps its GUI permissions.

When another Agent receives the computer work, the original task waits with `task_wait`. The receiving Agent verifies and transfers its output, then uses `task_finish` to return the result. The coordinator wakes the original waiting run and appends the terminal receipt, summary and actual return-transfer ledger to its existing context. The original Agent must still inspect the file and finish delivery through the source application.

Receipt consumption, context publication and the visible event share one PostgreSQL transaction. Manual pauses, cancellations and provider backoff remain in effect. The coordinator reports a disconnected cable or failed receiving task to the original task. Completion requires resolved delegated work and a read return receipt.

An unavailable target or missing Agent model produces a failed handoff receipt. Other assignments in the batch continue, and the next scheduling pass wakes the source task.

## Failure boundaries

Report account restrictions, sign-in requirements, failed sends and unknown action outcomes. Stop repeated sends after failure. Real-account attachment delivery is pending.

Android's notification shade is resolved through the exact visible primary-display window, its SystemUI owner and its privileged window type. The visible SystemUI window is the foreground evidence. Desktop notification text remains inert data in a bounded, private, atomically written guest inbox.

## Model request continuity

Persisted context and the execution ledger remain complete. The Responses request projection archives a tool call interrupted by a new user/system input as quoted evidence, with its actual result or unknown-result notice. For HTTP 400, inspect the provider’s request error.

## Verification and limits

Isolated PostgreSQL tests cover deduplication, source-key updates, concurrent scheduling, paused-task preservation, owner preemption, interrupted summaries, per-device wake and scheduling fairness. A blocked loopback phone request and a real PostgreSQL lock on desktop collection additionally verify independent admission, retention of the blocked device's supervision boundary, and cancellation without late admission. The fixtures measure admission from an already persisted notification. Guest inbox tests cover concurrent writers, file permissions, byte/count bounds and inert message content.

Handoff regressions additionally exercise early wake, once-only return consumption, transfer-ledger provenance, reverse-dispatch prevention, disconnected queued assignments, publication failure rollback and locked-row recovery.

Native model protocols and channel-specific delivery guarantees remain separate compatibility work.

---

# 通知驱动的工作

通知在探索关闭时也可唤醒设备对应的 Agent。普通消息自行判断、默认回复；工作请求可连接授权设备、处理附件、保留工作目录副本并回原会话交付。任务、事件和实际动作回执分别持久化，旧暂停任务保留，未知动作不自动重放。

手机采集、接管核对和手机入队保持原顺序；电脑采集与入队独立运行，各自只调度对应设备。某设备采集挂起不再拖住另一设备已有通知；采集仍为3秒周期并保留去重，新事件在同一轮即可入队，不再额外等待通知年龄。摘要转发仍独立，停止协调器后所有分支一同取消。

跨Agent交接时，原任务使用task_wait等待；接收端核验并传回产物后task_finish，系统自动唤醒原任务，将回执、摘要及真实回传文件账本接入原上下文，沿用原任务。回执与上下文在同一事务中发布，保留暂停、取消和服务商退避。原任务核对手机文件，再在来源会话交付。

目标不可用或缺少 Agent 模型时，该条交接生成失败回执，同批其他交接继续。下一轮调度唤醒原任务。

未连线时也能发现主人已安装的设备，并区分当前任务的GUI控制权与目标Agent的交接能力；任务控制数据线任一端即可连接文件通道，连线不会转移其他Agent的GUI控制权。

记录文件传输和消息发送的各自结果。登录失败或账号受限时报告原因，停止重复发送。微信实号办公回交待测。
