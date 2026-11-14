CREATE TABLE device_runtimes (
 device_id text PRIMARY KEY REFERENCES devices(id),
 provider text NOT NULL CHECK(provider='docker-desktop'),
 instance_ref text NOT NULL UNIQUE,
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE desktop_controls (
 device_id text PRIMARY KEY REFERENCES device_runtimes(device_id),
 epoch bigint NOT NULL DEFAULT 1,
 controller text NOT NULL DEFAULT '',
 touched_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE desktop_observations (
 id uuid PRIMARY KEY,
 device_id text NOT NULL REFERENCES device_runtimes(device_id),
 epoch bigint NOT NULL,
 captured_at timestamptz NOT NULL DEFAULT now(),
 title text NOT NULL,
 png_path text NOT NULL,
 sha256 text NOT NULL
);
CREATE INDEX desktop_observations_device_time ON desktop_observations(device_id,captured_at DESC);
CREATE TABLE desktop_actions (
 id uuid PRIMARY KEY,
 device_id text NOT NULL REFERENCES device_runtimes(device_id),
 run_id uuid REFERENCES runs(id),
 input_hash text NOT NULL,
 status text NOT NULL CHECK(status IN ('running','completed','unknown','failed')),
 result jsonb,
 created_at timestamptz NOT NULL DEFAULT now(),
 finished_at timestamptz
);
CREATE INDEX desktop_actions_device_time ON desktop_actions(device_id,created_at DESC);
CREATE TABLE device_links (
 id uuid PRIMARY KEY,
 source_device_id text NOT NULL REFERENCES devices(id),
 target_device_id text NOT NULL REFERENCES devices(id),
 connected boolean NOT NULL DEFAULT true,
 revision bigint NOT NULL DEFAULT 1,
 updated_at timestamptz NOT NULL DEFAULT now(),
 CHECK(source_device_id<target_device_id),
 UNIQUE(source_device_id,target_device_id)
);
CREATE TABLE device_transfers (
 id uuid PRIMARY KEY,
 link_id uuid NOT NULL REFERENCES device_links(id),
 link_revision bigint NOT NULL,
 run_id uuid REFERENCES runs(id),
 source_device_id text NOT NULL REFERENCES devices(id),
 target_device_id text NOT NULL REFERENCES devices(id),
 source_path text NOT NULL,
 target_path text NOT NULL,
 name text NOT NULL,
 bytes bigint NOT NULL DEFAULT 0,
 sha256 text NOT NULL DEFAULT '',
 status text NOT NULL CHECK(status IN ('pending','copying','completed','failed','unknown')),
 error text NOT NULL DEFAULT '',
 created_at timestamptz NOT NULL DEFAULT now(),
 finished_at timestamptz
);
CREATE INDEX device_transfers_link_time ON device_transfers(link_id,created_at DESC);
CREATE TABLE run_device_bindings (
 run_id uuid NOT NULL REFERENCES runs(id),
 device_id text NOT NULL REFERENCES devices(id),
 PRIMARY KEY(run_id,device_id)
);
INSERT INTO run_device_bindings(run_id,device_id)
 SELECT r.id,b.device_id FROM runs r JOIN agent_device_bindings b ON r.agent_id=b.agent_id ON CONFLICT DO NOTHING;

CREATE TABLE desktop_provision_jobs (
 id uuid PRIMARY KEY,
 device_id text NOT NULL REFERENCES devices(id),
 status text NOT NULL CHECK(status IN ('queued','running','completed','failed')),
 stage text NOT NULL DEFAULT '检查资源',
 error text NOT NULL DEFAULT '',
 cpus integer NOT NULL CHECK(cpus BETWEEN 1 AND 4),
 memory_mb integer NOT NULL CHECK(memory_mb BETWEEN 1536 AND 4096),
 template_id text NOT NULL CHECK(template_id='debian13-openbox'),
 created_at timestamptz NOT NULL DEFAULT now(),
 updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX desktop_one_install ON desktop_provision_jobs(device_id) WHERE status IN ('queued','running');
