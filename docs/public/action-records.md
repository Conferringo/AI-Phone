# Durable actions / 持久动作记录

The phone runner persists every accepted action before executing it. Each new action ID maps to a private record under `data/phone-runner/ledger.actions-v2/`. The record retains its request fingerprint, completion/result and unknown-outcome state. `ledger.json` holds control metadata and up to 128 cached entries. Evicted entries remain in the archive.

Only one runner may own this directory, enforced by an OS file lock. Writes use private files, atomic rename and file/directory sync. A matching old request returns the recorded result even after its control epoch changes. A changed request with the same ID is rejected. Unfinished/unknown actions are returned as unknown and are not re-executed; observe before planning another action. Corrupt archived records fail closed.

Legacy migration copies earlier inline records to `ledger.actions/` and keeps a private `ledger.pre-archive.json` reference before committing new metadata. The current runner reads both archive formats. Archive version 2 requires a runner that recognizes both.

Back up the entire user data directory while services are stopped, including metadata and the complete action archive together. Do not delete archive files to free a cache. Storage is retained until an explicit future retention/migration policy preserves deduplication; cross-host restore and full update rollback remain release gates.

## Desktop input and observations

Automated desktop actions request a bounded repaint observation. The guest samples raw frames locally, waits for a short quiet window after a meaningful visual change, and encodes only the final PNG. Sampling does not repeat input, call a model or write intermediate observations to PostgreSQL. Interactive streamed input retains its fast acknowledgement path; callers can explicitly request `wait_for_paint` when they need a post-input observation.

The protobuf `DesktopObservation` includes `frame_state` and `frame_wait_ms`. `STABLE` means the sampled image was briefly quiet, not that navigation or application work completed. `CHANGING` means the repaint budget expired during visual changes; `UNCHANGED` means no material visual change was observed. Neither state authorizes replaying a click or send. Check current controls, documents and application receipts to establish task completion.

The repaint policy samples at 40 ms, uses an 80 ms quiet window and an 800 ms wait budget, plus capture work. A foreground-title transition uses the full existing budget because window framing can precede document layout. Small caret changes do not keep the observation open. Ongoing animation returns the latest frame at the bound. The existing desktop action record retains the final observation and repaint metadata, so a matching retry restores the same receipt rather than executing input again. Older receipts have an unspecified frame state. No additional conversation store or database migration is required.

已接收动作先持久记账再执行；相同ID和参数返回原结果，未知结果不重放。整体备份须包含控制元数据和完整动作归档。电脑AI动作在客体内有界等待重绘，只返回最终画面；人工实时控制保留快速返回。任务结束时核对应用内的实际结果。
