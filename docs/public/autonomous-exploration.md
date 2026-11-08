# Self-directed exploration

The Agent chooses the application, activity and stopping point. It can move between apps, stay in one app, work across authorized devices or rest.

Phone and desktop exploration use one policy. Existing reply/install settings, device bindings, control epochs, owner steering and unknown-outcome receipts remain effective. Notifications and explicit owner work keep their priority.

## Continuity and scheduling

Normal execution yields in bounded slices. After 40 decision steps or ten minutes in an exploration window, the same run checkpoints its context, pending tools, plan and error counters, waits the configured interval, then continues. The next time budget starts on wake. Model-chosen natural stopping points can finish a cycle; the enabled scheduler can start another.

Long runs compact older tool and assistant messages into excerpts for the model. Owner and system constraints, each Skill's latest successful read and recent complete tool pairs stay in context. Archived records are historical evidence. Chat and execution records remain complete.

A paused owner task remains intact while explicitly enabled exploration gets its own scheduling lane. Re-enabling desktop exploration retires its old paused cycle while retaining all action receipts. Mode changes and scheduling lock the conversation, mode and runs in the same order. Stopping the mode cancels its active execution token while independently enabled notification work continues. Changing shared reply/install permissions revokes stale autonomous policy on both devices without switching off desktop exploration.

## Learning

The Agent can save a reusable Skill after it verifies a method through current observations, executed actions and a completion audit.

## Validation boundaries

Disposable database tests cover paused-owner coexistence, owner preemption, notification isolation, concurrent enable/schedule, stopping, device binding checks and install settings. A real worker test verifies that resource yielding preserves context, pending work, plans and retry counters without executing a pending action. Unit tests cover restart, long sleeps, elapsed time and step boundaries.

Report platform restrictions in the task trace. See [release status](release-status.md) for real-account delivery and installation checks.

---

# 无指向自主探索

Agent 自行选择应用、活动和停下的时机。可以跨应用，在同一应用继续，协作使用已授权设备，也可以休息。

运行切片保留同一任务的上下文、待执行工具和进度。旧暂停任务与新探索分别调度，通知、自主探索和人工接管各自保持有效。
