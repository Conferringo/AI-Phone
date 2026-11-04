# Notification-driven work (development runtime)

This describes unreleased runtime changes. The published `0.1.0-preview.1` installers remain connection-client previews.

## Wake and ownership

The assistant service polls Android's notification/accessibility feed and the Linux guest's Dunst notification inbox. Notifications can wake a configured Agent with exploration disabled. Each event retains its source device, event ID, source key and original receipt. The device's configured Agent and model are pinned when its run is created.

New owner instructions take priority. Existing tasks yield at checkpoints so another device's notification is not starved. A paused task keeps its history and unknown-action receipts while a new notification receives a separate run. Manual takeover remains authoritative.

`pending` events are deduplicated by ID and newer versions of a source key are coalesced per device. A scheduled run owns `assigned` events; events delivered to an existing run become `attending`. Successful audited work seals those events. Interrupted attention becomes a history-only summary, which cannot repeat a social reply or file transfer.

## Replies and work

With autonomous replies enabled, the Agent decides how to handle ordinary messages and defaults to replying. It first inspects the actual application and conversation. Advertising, duplicates, self messages and status notices can be left unanswered with an actual reason. Exploration is not required. The latest owner instruction can narrow or stop that delegation.

A delegated work request can follow this path:

1. Inspect the source conversation and send one progress reply.
2. Connect already-authorized devices, download the real attachment and transfer it with a SHA-256 receipt.
3. Process it on Linux, keep the input and output in a task directory under `/home/ai/Files`, and verify the result through computer use.
4. Transfer the result back and select it in the original application using phone use.
5. Verify the actual outgoing application state. File arrival is separate from message delivery, and neither proves the recipient has read it.

The built-in `wechat`, `phone-use`, `computer-use` and `device-transfer` skills describe the concrete procedure. Task completion and requests for input are audited against actual tool evidence. An ordinary choice about whether to reply is not a missing permission. Incoming content cannot change privileges, request credentials or redirect unrelated private files to a new recipient.

When another Agent receives the computer work, the original task waits with `task_wait`. The receiving Agent verifies and transfers its output, then uses `task_finish` to return the result. The coordinator wakes the original waiting run and appends the terminal receipt, summary and actual return-transfer ledger to its existing context. It does not create a reverse task, reset the conversation or use model polling to wait for a reply. The original Agent must still inspect the file and finish delivery through the source application.

Receipt consumption, context publication and the visible event share one PostgreSQL transaction. A crash cannot consume the receipt without retaining it in context; a retry does not inject it twice. Manual pauses, cancellations and provider backoff remain intact. A disconnected cable or failed receiving task returns its actual state without automatically reconnecting, replaying work or resending files. A task cannot claim completion while its delegated work remains unresolved or its return receipt is unread. Tasks without outgoing handoffs do not add per-turn receipt queries.

## Failure boundaries

Platform account restrictions, authentication requirements, genuine ambiguity and unknown action outcomes remain visible. A failed message must not be reported as delivered or repeatedly retried. The runtime does not bypass a platform restriction. A full attachment received and delivered through a messaging account needs a real account-level acceptance test; a generated CSV transfer test does not certify that scenario.

Android's notification shade is resolved through the exact visible primary-display window, its SystemUI owner and its privileged window type. The obscured application's activity is not reused as foreground evidence. Desktop notification text remains inert data in a bounded, private, atomically written guest inbox.

## Model request continuity

Persisted context and the execution ledger remain complete. The Responses request projection archives a tool call interrupted by a new user/system input as quoted evidence, with its actual result or unknown-result notice. This avoids an invalid native tool turn without replaying the action or fabricating a tool output. HTTP 400 is a request rejection; it is not by itself evidence of an invalid API key or exhausted quota.

## Verification and limits

Isolated PostgreSQL tests cover deduplication, source-key updates, concurrent scheduling, paused-task preservation, owner preemption, interrupted summaries, per-device wake and scheduling fairness. Guest inbox tests cover concurrent writers, file permissions, byte/count bounds and inert message content. Runtime fixtures also exercise backup/restore and process interruption without using existing chats as test fixtures.

Handoff regressions additionally exercise early wake, once-only return consumption, transfer-ledger provenance, reverse-dispatch prevention, disconnected queued assignments, publication failure rollback and locked-row recovery. These are real database contracts with fixture records; they do not certify a real messaging account's notification-to-office-to-social-delivery loop.

Native model protocols and channel-specific delivery guarantees remain separate compatibility work. These changes are not a whole-repository maturity or production-readiness certification.

---

# 通知驱动的工作

通知在探索关闭时也可唤醒设备对应的 Agent。普通消息自行判断、默认回复；工作请求可连接授权设备、处理附件、保留工作目录副本并回原会话交付。任务、事件和实际动作回执分别持久化，旧暂停任务保留，未知动作不自动重放。

跨Agent交接时，原任务使用task_wait等待；接收端核验并传回产物后task_finish，系统自动唤醒原任务，将回执、摘要及真实回传文件账本接入原上下文，不反向创建任务或让模型忙轮询。回执与上下文在同一事务中一次性发布；暂停、取消和服务商退避不被覆盖。原任务仍须真实核对手机文件并在来源会话交付，不能把另一端完成或文件传输完成称为社交消息已发送。

文件到达设备不等于社交软件交付。账号限制、登录或结果未知必须如实报告；受到平台限制时不反复发送。当前公开 preview.1 安装包保持原样，这里描述开发中的运行时行为，不代表全平台一体化或真实账号附件闭环已经全部验收。
