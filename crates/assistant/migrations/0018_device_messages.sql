ALTER TABLE runs DROP CONSTRAINT IF EXISTS runs_origin_check;
ALTER TABLE runs ADD CONSTRAINT runs_origin_check CHECK(origin IN ('user','exploration','notification','desktop-exploration','device-handoff'));
CREATE TABLE device_messages (
 id uuid PRIMARY KEY,
 link_id uuid NOT NULL REFERENCES device_links(id),
 link_revision bigint NOT NULL,
 source_device_id text NOT NULL REFERENCES devices(id),
 target_device_id text NOT NULL REFERENCES devices(id),
 source_run_id uuid REFERENCES runs(id),
 target_run_id uuid REFERENCES runs(id),
 text text NOT NULL,
 status text NOT NULL DEFAULT 'queued' CHECK(status IN ('queued','running','completed','failed','canceled')),
 error text NOT NULL DEFAULT '',
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX device_messages_pending ON device_messages(created_at) WHERE status='queued';
