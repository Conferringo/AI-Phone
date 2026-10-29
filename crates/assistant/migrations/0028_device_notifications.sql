-- Preserve old tasks and their receipts. A paused task cannot suppress new
-- environmental work; user steering still cancels background work first.
DROP INDEX runs_one_active_per_conversation;
DROP INDEX runs_one_readonly_notification_per_conversation;
CREATE UNIQUE INDEX runs_one_active_per_conversation ON runs(conversation_id)
 WHERE status IN ('queued','running','paused') AND origin<>'notification';
CREATE UNIQUE INDEX runs_one_notification_per_conversation ON runs(conversation_id)
 WHERE status IN ('queued','running') AND origin='notification';

-- Retain the historical table name and all event IDs, assignments and receipts.
ALTER TABLE phone_notifications ADD COLUMN device_id text NOT NULL DEFAULT 'phone-local-01';
CREATE INDEX notifications_device_pending ON phone_notifications(device_id,received_at)
 WHERE status IN ('pending','pending-summary');
