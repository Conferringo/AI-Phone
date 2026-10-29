-- Preserve paused user work while an independent read-only notification is summarized.
-- A new user instruction locks the conversation and cancels this summary before taking control.
DROP INDEX runs_one_active_per_conversation;
CREATE UNIQUE INDEX runs_one_active_per_conversation ON runs(conversation_id)
 WHERE status IN ('queued','running','paused')
 AND NOT (origin='notification' AND background_read_only);
CREATE UNIQUE INDEX runs_one_readonly_notification_per_conversation ON runs(conversation_id)
 WHERE status IN ('queued','running','paused')
 AND origin='notification' AND background_read_only;
