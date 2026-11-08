-- Accepted voice revisions are append-only input metadata, not another chat store.
CREATE TABLE chat_voice_inputs (
  request_id uuid PRIMARY KEY,
  message_id uuid NOT NULL UNIQUE REFERENCES messages(id) ON DELETE CASCADE,
  conversation_id uuid NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  session_id uuid NOT NULL,
  utterance_id text NOT NULL CHECK (octet_length(utterance_id) BETWEEN 1 AND 128),
  revision bigint NOT NULL CHECK (revision BETWEEN 1 AND 4294967295),
  literal text NOT NULL CHECK (octet_length(literal) BETWEEN 1 AND 48000),
  corrects_request_id uuid REFERENCES chat_voice_inputs(request_id) DEFERRABLE INITIALLY DEFERRED,
  UNIQUE (conversation_id,session_id,utterance_id,revision)
);
ALTER TABLE runs ADD COLUMN input_quiet_ms smallint NOT NULL DEFAULT 700
  CHECK (input_quiet_ms IN (0,700));
