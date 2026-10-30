-- Exploration has one per-device pause control. Capabilities no longer need setup.
-- This one-time upgrade enables the requested defaults; later user pauses persist.
ALTER TABLE proactive_state ALTER COLUMN exploring SET DEFAULT true;
ALTER TABLE proactive_state ALTER COLUMN wechat_watch SET DEFAULT true;
UPDATE proactive_state SET exploring=true, notifications_enabled=true,
 forward_notifications=true, autonomous_reply=true, autonomous_install=true,
 wechat_watch=true, control_epoch=0, revision=revision+1,
 next_cycle_at=now(), last_error='', failure_streak=0;
ALTER TABLE desktop_exploration ALTER COLUMN enabled SET DEFAULT true;
ALTER TABLE desktop_exploration ALTER COLUMN interval_seconds SET DEFAULT 45;
INSERT INTO desktop_exploration(device_id)
 SELECT r.device_id FROM device_runtimes r JOIN devices d ON d.id=r.device_id
 WHERE d.runtime_state='managed' AND d.id<>'phone-local-01'
 ON CONFLICT DO NOTHING;
UPDATE desktop_exploration e SET enabled=true, interval_seconds=45,
 next_cycle_at=now(), last_error=''
 FROM devices d WHERE d.id=e.device_id AND d.runtime_state='managed';
