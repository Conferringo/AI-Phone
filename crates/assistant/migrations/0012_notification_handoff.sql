-- A notification interrupted by user work can be summarized later without replaying social replies.
ALTER TABLE runs ADD COLUMN background_read_only boolean NOT NULL DEFAULT false;
DROP INDEX phone_notifications_pending;
DROP INDEX phone_notifications_pending_key;
CREATE INDEX phone_notifications_pending ON phone_notifications(received_at) WHERE status IN ('pending','pending-summary');
CREATE INDEX phone_notifications_pending_key ON phone_notifications(notification_key,posted_at,event_id) WHERE status IN ('pending','pending-summary');
