# Liberty

Liberty begins without a preset application or goal. The Agent can look at its device, choose something to investigate, stay with an activity, or rest. When an interest becomes a multi-step goal, the Agent can record it in its plan. It can revise that goal when it learns something new, encounters a blocker, or finds a better direction.

Liberty uses the same policy on phone and desktop. The device's existing reply and installation permissions apply. Notifications, explicit owner work and manual takeover have priority.

## Continuity and scheduling

Normal execution yields in bounded slices. After 40 decision steps or ten minutes in an exploration window, the same run checkpoints its context, pending tools, plan and error counters, waits the configured interval, then continues. The next time budget starts on wake. Model-chosen natural stopping points can finish a cycle; the enabled scheduler can start another.

Long runs compact older tool and assistant messages into excerpts for the model. The latest saved Liberty goal, owner and system constraints, each Skill's latest successful read and recent complete tool pairs stay in context. The Agent can observe again and decide whether its saved goal still fits.

One Liberty control applies to both devices. An active Task & Goal pauses Liberty. Stopping Liberty ends its active cycle and leaves notification handling enabled.

## Harness collaboration

When a direction benefits from another device, the Agent can check its device bindings and connect the data cable. The selected Agent can operate either device, transfer files and delegate a step to the other device. The returned result wakes the original task.

The Agent can organize shared work files, search the knowledge base, and save reusable knowledge from its work. It can set a simulated point or route when the owner has enabled simulated location and the current task controls every affected guest.

## Learning

The Agent can save a reusable Skill after it verifies a method through current observations, executed actions and a completion audit.

## Validation boundaries

Disposable database tests cover paused-owner coexistence, owner preemption, notification isolation, concurrent enable/schedule, stopping, device binding checks and install settings. A real worker test verifies that resource yielding preserves context, pending work, plans and retry counters without executing a pending action. Unit tests cover restart, long sleeps, elapsed time and step boundaries.

Report platform restrictions in the task trace. See [release status](release-status.md) for real-account delivery and installation checks.

---

# Liberty

Liberty 启动时没有预设应用或目标。Agent 可以先观察设备，自行选择兴趣、继续手头的活动或休息。有了多步目标时可以记下计划，观察到新线索、遇到阻碍或找到更好的方向时可以修订。

运行切片保存同一任务的上下文、待执行工具和进度。压缩历史时保留最近保存的目标，供 Agent 根据新观察判断是否继续。通知、主人明确指令和人工接管优先。

Liberty 在两台设备间共享。启动「任务与目标」时会暂停 Liberty。Agent 可操作两台设备、连接数据线、传输文件或委派子任务。结果回传后会唤醒原任务。Agent 可以整理工作文件并把可复用内容写入知识库。用户开启模拟位置后，当前任务获准控制所有受影响设备时，Agent 可以调整模拟点或路线。
