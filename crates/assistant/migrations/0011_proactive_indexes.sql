CREATE INDEX phone_notifications_run ON phone_notifications(run_id) WHERE run_id IS NOT NULL;
CREATE INDEX phone_notifications_pending_key ON phone_notifications(notification_key,posted_at,event_id) WHERE status='pending';
CREATE INDEX proactive_reports_pending ON proactive_reports(created_at) WHERE status IN ('pending','queued');
CREATE INDEX runs_background_recent ON runs(created_at DESC) WHERE origin<>'user';
