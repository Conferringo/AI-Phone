ALTER TABLE runs ADD COLUMN origin text NOT NULL DEFAULT 'user' CHECK(origin IN ('user','exploration','notification'));
ALTER TABLE runs ADD COLUMN trigger_input text NOT NULL DEFAULT '';
ALTER TABLE runs ADD COLUMN background_finalized boolean NOT NULL DEFAULT false;
CREATE INDEX runs_background_unfinalized ON runs(status) WHERE origin<>'user' AND NOT background_finalized;
CREATE TABLE proactive_state (
 owner_id text PRIMARY KEY DEFAULT 'local-user', revision bigint NOT NULL DEFAULT 1,
 exploring boolean NOT NULL DEFAULT false,
 notifications_enabled boolean NOT NULL DEFAULT true,
 forward_notifications boolean NOT NULL DEFAULT true,
 autonomous_reply boolean NOT NULL DEFAULT true,
 autonomous_install boolean NOT NULL DEFAULT true,
 interval_seconds integer NOT NULL DEFAULT 45 CHECK(interval_seconds BETWEEN 15 AND 3600),
 next_cycle_at timestamptz NOT NULL DEFAULT now(),
 notifications_since timestamptz NOT NULL DEFAULT now(),
 listener_connected boolean NOT NULL DEFAULT false,
 listener_checked_at timestamptz, control_epoch bigint NOT NULL DEFAULT 0,
 last_error text NOT NULL DEFAULT '', failure_streak integer NOT NULL DEFAULT 0
);
INSERT INTO proactive_state(owner_id) VALUES('local-user');
CREATE TABLE phone_notifications (
 event_id text PRIMARY KEY, notification_key text NOT NULL, package_name text NOT NULL,
 app_name text NOT NULL, title text NOT NULL, body text NOT NULL,
 posted_at timestamptz NOT NULL, received_at timestamptz NOT NULL DEFAULT now(),
 run_id uuid REFERENCES runs(id), status text NOT NULL DEFAULT 'pending'
);
CREATE INDEX phone_notifications_pending ON phone_notifications(received_at) WHERE status='pending';
CREATE TABLE proactive_reports (
 run_id uuid PRIMARY KEY REFERENCES runs(id), request_id uuid NOT NULL UNIQUE,
 status text NOT NULL DEFAULT 'pending', detail text NOT NULL DEFAULT '',
 created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now()
);
