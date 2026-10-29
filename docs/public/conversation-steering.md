# Continuous conversation / 连续对话

AI Phone accepts new input while the Agent is working. An input receipt means the message is saved, not that the model has read or completed it.

## Runtime contract

1. The composer immediately captures an immutable message and receipt ID in a local outbox. New drafts and attachments belong to the next message. A failed request retains the same receipt for retry; later messages stay in FIFO order.
2. PostgreSQL stores every accepted user message. Conversation locks establish accepted order; a request lock and existing run/input receipts prevent duplicate execution across retries. `messages.ordinal` orders the transcript independently of transaction start time.
3. A 700 ms quiet window combines short bursts before requesting the model. Input during a model request cancels only that obsolete request; it does not cancel the task or a running device action. The adapter releases the HTTP response on cancellation. The provider may still bill already-generated tokens.
4. At a tool boundary, completed results remain recorded. Unstarted old calls receive explicit non-executed results. A running operation with an unknown result pauses for observation and is never automatically replayed.
5. New assistant fragments get new message IDs. Previous text and tool events retain their original positions. The frontend scopes execution traces by message and ignores older history snapshots after a newer receipt.
6. Context projection preserves user constraints and tool/result pairing. “Continue” inherits the current task's evidence requirements; an explicit correction takes priority. This is not a claim of unlimited context or permanent recall across unrelated tasks.

## 用户体验

可以连续补充，不必等回复结束。普通回车换行，Command/Ctrl + 回车发送。新草稿不会被上一条请求的回执清空。发送失败时消息仍在本机，点击「重试」继续；刷新后待发送消息使用原标识恢复。已显示的回复、可核验的进展和操作记录保留，不展示或虚构隐藏思维链。

## Reference decisions

- [Codex App Server](https://learn.chatgpt.com/docs/app-server): distinguish steering an active turn from interrupting it; retain turn items and events.
- [Claude Code interactive mode](https://code.claude.com/docs/en/interactive-mode): accept queued input while working and deliver it at appropriate execution boundaries.
- [OpenClaw steering queue](https://docs.openclaw.ai/concepts/queue-steering): preserve FIFO input, paired tool outcomes and the distinction between a receipt and runtime consumption.
- [Hermes tips](https://hermes-agent.nousresearch.com/docs/guides/tips): interruption and redirection are normal conversation operations; multiline drafts are buffered.

These are independently implemented behavior contracts based on public documentation, not copied harness code or claims about the private internals of Dots/Grok.

## Verification scope

See the current batch in `docs/project-tracker.md`. Regression coverage includes concurrent input, repeated receipts, completion races, slow HTTP model interruption, unknown operation results, compaction, reload recovery and message-level rendering. Real DeepSeek latency and physical device actions are not simulated as proof of this test suite.
