-- Speech configuration belongs to an Agent, independently of chat probes/revisions.
-- Credentials remain in the private database, never in registry/event projections.
CREATE TABLE agent_voice_profiles (
  agent_id uuid PRIMARY KEY REFERENCES agents(id) ON DELETE CASCADE,
  revision bigint NOT NULL CHECK (revision > 0),
  configuration bytea NOT NULL
);
-- Corrections form a single append-only chain; conversation locking owns writes.
CREATE UNIQUE INDEX chat_voice_inputs_one_correction
  ON chat_voice_inputs(corrects_request_id) WHERE corrects_request_id IS NOT NULL;
