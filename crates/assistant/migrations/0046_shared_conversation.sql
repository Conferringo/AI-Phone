DROP INDEX runs_one_active_per_conversation;
DROP INDEX runs_one_exploration_per_conversation;
DROP INDEX runs_one_notification_per_conversation;
DROP INDEX runs_current_conversation;

-- Retain the active goal first, then the newest run in each shared lane.
WITH ranked AS (
    SELECT id, row_number() OVER (
        PARTITION BY CASE WHEN origin='desktop-exploration' THEN 'exploration' ELSE origin END,
                     CASE WHEN origin='device-handoff' THEN device_id END
        ORDER BY (id IS NOT DISTINCT FROM (SELECT goal_run_id FROM proactive_state WHERE owner_id='local-user')) DESC,
                 GREATEST(input_at, created_at) DESC, id DESC
    ) position
    FROM runs
    WHERE status IN ('queued','running','paused')
      AND (origin <> 'notification' OR status <> 'paused')
)
UPDATE runs r
SET status='canceled', finished_at=now(), next_wake_at=NULL
FROM ranked WHERE ranked.id=r.id AND ranked.position>1;

UPDATE phone_notifications n
SET status='pending-summary'
FROM runs r WHERE r.id=n.run_id AND r.status='canceled' AND n.status='assigned';

ALTER TABLE runs DROP CONSTRAINT runs_origin_check;
UPDATE runs SET origin='exploration' WHERE origin='desktop-exploration';
ALTER TABLE runs ADD CONSTRAINT runs_origin_check CHECK(origin IN ('user','exploration','notification','device-handoff'));

DO $$
DECLARE canonical uuid;
BEGIN
    SELECT id INTO STRICT canonical FROM conversations WHERE owner_id='local-user' AND device_id='phone-local-01';
    UPDATE messages SET conversation_id=canonical WHERE conversation_id<>canonical;
    UPDATE runs SET conversation_id=canonical WHERE conversation_id<>canonical;
    UPDATE chat_voice_inputs SET conversation_id=canonical WHERE conversation_id<>canonical;
    DELETE FROM conversations WHERE owner_id='local-user' AND id<>canonical;
END $$;

DROP INDEX conversation_per_device;
ALTER TABLE conversations DROP COLUMN device_id;
ALTER TABLE conversations ADD CONSTRAINT conversation_per_owner UNIQUE(owner_id);

CREATE UNIQUE INDEX runs_one_active_per_conversation ON runs(conversation_id)
 WHERE status IN ('queued','running','paused') AND origin='user';
CREATE UNIQUE INDEX runs_one_exploration_per_conversation ON runs(conversation_id)
 WHERE status IN ('queued','running','paused') AND origin='exploration';
CREATE UNIQUE INDEX runs_one_notification_per_conversation ON runs(conversation_id)
 WHERE status IN ('queued','running') AND origin='notification';
CREATE UNIQUE INDEX runs_one_handoff_per_device ON runs(device_id)
 WHERE status IN ('queued','running','paused') AND origin='device-handoff';

CREATE INDEX runs_current_conversation ON runs (
    conversation_id,
    (status IN ('queued','running','paused') AND origin='user') DESC,
    (status IN ('queued','running','paused')) DESC,
    GREATEST(input_at,finished_at,created_at) DESC,
    id DESC
);

UPDATE proactive_state
SET exploring=true
WHERE goal_run_id IS NULL AND EXISTS (SELECT 1 FROM desktop_exploration WHERE enabled);
DROP TABLE desktop_exploration;
