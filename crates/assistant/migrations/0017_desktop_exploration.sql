ALTER TABLE runs DROP CONSTRAINT IF EXISTS runs_origin_check;
ALTER TABLE runs ADD CONSTRAINT runs_origin_check CHECK(origin IN ('user','exploration','notification','desktop-exploration'));
CREATE TABLE desktop_exploration (
 device_id text PRIMARY KEY REFERENCES device_runtimes(device_id),
 enabled boolean NOT NULL DEFAULT false,
 interval_seconds integer NOT NULL DEFAULT 60 CHECK(interval_seconds BETWEEN 30 AND 3600),
 next_cycle_at timestamptz NOT NULL DEFAULT now(),
 last_error text NOT NULL DEFAULT ''
);
