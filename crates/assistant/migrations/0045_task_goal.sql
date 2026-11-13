ALTER TABLE proactive_state
    ADD COLUMN goal text NOT NULL DEFAULT '',
    ADD COLUMN goal_run_id uuid REFERENCES runs(id) ON DELETE SET NULL;
