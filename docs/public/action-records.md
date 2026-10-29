# Durable actions / 持久动作记录

The phone runner persists every accepted action before executing it. Each action ID maps to a SHA256-sharded private record under `data/phone-runner/ledger.actions/`. The record retains its request fingerprint, completion/result and unknown-outcome state. `ledger.json` holds control metadata and at most 128 cached entries; evicting a cache entry never deletes the durable result. There is no 2000-action hard stop.

Only one runner may own this directory, enforced by an OS file lock. Writes use private files, atomic rename and file/directory sync. A matching old request returns the recorded result even after its control epoch changes. A changed request with the same ID is rejected. Unfinished/unknown actions are returned as unknown and are not re-executed; observe before planning another action. Corrupt archived records fail closed.

Legacy migration copies every old record, preserves a private `ledger.pre-archive.json` reference and only then writes the new metadata. New files use `cached_actions`; old binaries that require `actions` refuse to load them. This intentionally blocks an unsafe downgrade that would ignore the archive. The pre-migration copy is **not** a safe live rollback after new actions have happened.

Back up the entire user data directory while services are stopped, including metadata and the complete action archive together. Do not delete archive files to free a cache. Storage is retained until an explicit future retention/migration policy preserves deduplication; cross-host restore and full update rollback remain release gates. This is not an exactly-once guarantee for an external app or network service.

本轮已验证：2101条完成/未知记录的迁移、坏记录拒绝、单进程锁、缓存淘汰不删除记录、跨epoch冷记录不调用ADB、旧读取器拒绝新格式。现有1701条记录逐项内容保持；真实手机动作完成后，重复请求与runner重启后的请求均返回去重结果。没有通过删记录或单纯提高上限来绕过问题。
