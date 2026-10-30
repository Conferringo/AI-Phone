ALTER TABLE runs ADD COLUMN steering jsonb NOT NULL DEFAULT '[]';
CREATE TABLE run_inputs (
  request_id uuid PRIMARY KEY,
  run_id uuid NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
  message_id uuid NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  fingerprint text NOT NULL
);
CREATE INDEX run_inputs_run ON run_inputs(run_id);
