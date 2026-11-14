-- Location is private user state. The binary payload follows location.proto.
CREATE TABLE simulated_location (
  owner_id text PRIMARY KEY CHECK (owner_id = 'local-user'),
  revision bigint NOT NULL CHECK (revision >= 0),
  state bytea NOT NULL,
  updated_at timestamptz NOT NULL DEFAULT now()
);
