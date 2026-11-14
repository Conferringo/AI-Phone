CREATE TABLE app_learning_observations (
 observation_id text PRIMARY KEY,
 run_id uuid NOT NULL REFERENCES runs(id),
 device_id text NOT NULL REFERENCES devices(id),
 app_id text NOT NULL,
 event_id uuid NOT NULL REFERENCES harness_events(id),
 after_action boolean NOT NULL DEFAULT false,
 observed_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX app_learning_device_app ON app_learning_observations(device_id,app_id,observed_at DESC);
CREATE INDEX app_learning_run ON app_learning_observations(run_id,app_id);
CREATE TABLE app_experiences (
 id uuid PRIMARY KEY,
 run_id uuid NOT NULL REFERENCES runs(id),
 device_id text NOT NULL REFERENCES devices(id),
 app_id text NOT NULL,
 title text NOT NULL,
 procedure text NOT NULL,
 outcome text NOT NULL CHECK(outcome IN ('verified','blocked')),
 limitations text NOT NULL DEFAULT '',
 observation_ids text[] NOT NULL,
 fingerprint text NOT NULL,
 approved boolean NOT NULL DEFAULT false,
 created_at timestamptz NOT NULL DEFAULT now(),
 UNIQUE(run_id,fingerprint)
);
CREATE INDEX app_experiences_app ON app_experiences(device_id,app_id,created_at DESC);
