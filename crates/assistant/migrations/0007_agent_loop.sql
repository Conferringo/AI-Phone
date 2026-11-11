ALTER TABLE runs ADD COLUMN next_wake_at timestamptz;
ALTER TABLE runs ADD COLUMN loop_state jsonb NOT NULL DEFAULT '{}';
CREATE INDEX runs_due ON runs(next_wake_at,created_at,id) WHERE status='queued';
