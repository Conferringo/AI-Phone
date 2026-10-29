-- Configuration registry only. Registered devices do not imply provisioned runtimes.
CREATE TABLE model_connections (
  id uuid PRIMARY KEY,
  owner_id text NOT NULL DEFAULT 'local-user',
  name text NOT NULL CHECK(length(name) BETWEEN 1 AND 80),
  revision bigint NOT NULL DEFAULT 1 CHECK(revision > 0),
  model_revision bigint NOT NULL CHECK(model_revision >= 0),
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX model_connections_owner ON model_connections(owner_id,created_at,id);
CREATE TABLE agents (
  id uuid PRIMARY KEY,
  owner_id text NOT NULL DEFAULT 'local-user',
  name text NOT NULL CHECK(length(name) BETWEEN 1 AND 80),
  revision bigint NOT NULL DEFAULT 1 CHECK(revision > 0),
  model_connection_id uuid NOT NULL REFERENCES model_connections(id),
  instructions text NOT NULL DEFAULT '' CHECK(length(instructions) <= 8000),
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX agents_owner ON agents(owner_id,created_at,id);
CREATE INDEX agents_model_connection ON agents(model_connection_id);
CREATE TABLE devices (
  id text PRIMARY KEY,
  owner_id text NOT NULL DEFAULT 'local-user',
  name text NOT NULL CHECK(length(name) BETWEEN 1 AND 80),
  kind text NOT NULL CHECK(kind IN ('phone','desktop')),
  system text NOT NULL,
  runtime_state text NOT NULL CHECK(runtime_state IN ('managed','registered')),
  revision bigint NOT NULL DEFAULT 1 CHECK(revision > 0),
  default_agent_id uuid REFERENCES agents(id),
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX devices_owner ON devices(owner_id,created_at,id);
CREATE INDEX devices_default_agent ON devices(default_agent_id);
CREATE TABLE agent_device_bindings (
  agent_id uuid NOT NULL REFERENCES agents(id),
  device_id text NOT NULL REFERENCES devices(id),
  created_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY(agent_id,device_id)
);
CREATE INDEX bindings_device ON agent_device_bindings(device_id,agent_id);
ALTER TABLE devices ADD CONSTRAINT device_default_binding
  FOREIGN KEY(default_agent_id,id) REFERENCES agent_device_bindings(agent_id,device_id)
  DEFERRABLE INITIALLY DEFERRED;
CREATE TABLE registry_meta (
  owner_id text PRIMARY KEY,
  legacy_revision bigint NOT NULL DEFAULT 0
);
-- Immutable run snapshots survive later configuration and binding edits.
ALTER TABLE runs ADD COLUMN agent_id uuid REFERENCES agents(id);
ALTER TABLE runs ADD COLUMN agent_revision bigint NOT NULL DEFAULT 0;
ALTER TABLE runs ADD COLUMN agent_name text NOT NULL DEFAULT '';
ALTER TABLE runs ADD COLUMN agent_instructions text NOT NULL DEFAULT '';
ALTER TABLE runs ADD COLUMN device_id text NOT NULL DEFAULT 'phone-local-01';
ALTER TABLE runs ADD COLUMN model_connection_id uuid REFERENCES model_connections(id);
CREATE INDEX runs_agent ON runs(agent_id,created_at,id);
CREATE INDEX runs_model_connection ON runs(model_connection_id);
