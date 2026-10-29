# Self-directed exploration

This is development-runtime behavior. Published preview.1 client binaries are unchanged.

Exploration has no prescribed application, topic or deliverable. The Agent chooses what to do. It can switch applications, continue an activity in one application, use already-authorized devices together, or rest. Application history is memory, not a rotation schedule. There is no two-round application cooldown, required learning exercise or per-round Skill-writing quota.

Phone and desktop exploration use one policy. Existing reply/install settings, device bindings, control epochs, owner steering and unknown-outcome receipts remain effective. Ordinary decisions do not require the owner to invent a goal for the Agent. Notifications and explicit owner work keep their priority.

## Continuity and scheduling

Normal execution yields in bounded slices. After 40 decision steps or ten minutes in an exploration window, the same run checkpoints its context, pending tools, plan and error counters, waits the configured interval, then continues. This resource window does not cancel the activity or demand a different App. Its next clock begins on wake, so long waits do not cause an endless yield loop. Model-chosen natural stopping points can finish a cycle; the enabled scheduler can start another.

A paused owner task remains intact while explicitly enabled exploration gets its own scheduling lane. An unrelated notification's platform restriction does not disable exploration. Re-enabling desktop exploration retires its old paused cycle while retaining all action receipts. Mode changes and scheduling lock the conversation, mode and runs in the same order. Stopping the mode cancels its active execution token while independently enabled notification work continues. Changing shared reply/install permissions revokes stale autonomous policy on both devices without switching off desktop exploration.

## Learning

Reusable methods may be saved when the Agent actually discovers them. There is no requirement to generate a new method or document every round. App experience still needs current, same-device observations, action verification and an independent completion audit before it becomes a reusable Skill. Free exploration does not weaken evidence requirements.

## Validation boundaries

Disposable database tests cover paused-owner coexistence, owner preemption, notification isolation, concurrent enable/schedule, stopping, device binding checks and install settings. A real worker test verifies that resource yielding preserves context, pending work, plans and retry counters without executing a pending action. Unit tests cover restart, long sleeps, elapsed time and step boundaries.

A model may still make poor choices or encounter inaccessible applications. Actual device operation and account-level delivery are separate acceptance checks. Platform restrictions are reported rather than treated as successful actions. Physical fresh-host installation and four-platform standalone hosts are not certified by these runtime tests.

---

# 无指向自主探索

不指定App、主题或产出，让Agent自己决定做什么。可以跨应用、在同一App继续感兴趣的活动、协作使用已授权设备，也可以休息。不要求轮换应用、每轮学习新能力或提交心得。

运行切片保留同一run的上下文、待执行工具和进度，不因额度到达而取消活动。旧暂停任务不妨碍明确开启的新探索；某条通知受阻不再关闭无关探索。停止、接管、设备绑定和真实动作回执继续生效。

本页描述开发中的运行时能力，preview.1客户端安装包保持不变，不代表完整跨平台独立宿主或真实社交平台交付已全部验收。
