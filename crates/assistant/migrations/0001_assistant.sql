CREATE TABLE conversations (
  id uuid PRIMARY KEY,
  owner_id text NOT NULL DEFAULT 'local-user',
  title text NOT NULL DEFAULT '',
  created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE attachments (
  id uuid PRIMARY KEY,
  owner_id text NOT NULL DEFAULT 'local-user',
  name text NOT NULL,
  media_type text NOT NULL,
  size_bytes bigint NOT NULL,
  storage_key text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE messages (
  id uuid PRIMARY KEY,
  conversation_id uuid NOT NULL REFERENCES conversations(id),
  role text NOT NULL CHECK(role IN ('user','assistant')),
  text text NOT NULL DEFAULT '',
  attachment_ids uuid[] NOT NULL DEFAULT '{}',
  created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX messages_conversation_cursor ON messages(conversation_id,created_at,id);
CREATE TABLE runs (
  id uuid PRIMARY KEY,
  conversation_id uuid NOT NULL REFERENCES conversations(id),
  assistant_message_id uuid NOT NULL REFERENCES messages(id),
  request_id uuid NOT NULL UNIQUE,
  request_fingerprint text NOT NULL,
  status text NOT NULL CHECK(status IN ('queued','running','paused','succeeded','failed','canceled')),
  context jsonb NOT NULL DEFAULT '[]',
  pending_tools jsonb NOT NULL DEFAULT '[]',
  step integer NOT NULL DEFAULT 0,
  control_epoch bigint NOT NULL DEFAULT 0,
  error text NOT NULL DEFAULT '',
  answer text NOT NULL DEFAULT '',
  created_at timestamptz NOT NULL DEFAULT now(),
  heartbeat_at timestamptz,
  finished_at timestamptz
);
CREATE UNIQUE INDEX runs_one_active_per_conversation ON runs(conversation_id) WHERE status IN ('queued','running','paused');
CREATE INDEX runs_queued ON runs(created_at,id) WHERE status='queued';
CREATE TABLE memories (
  id uuid PRIMARY KEY,
  owner_id text NOT NULL DEFAULT 'local-user',
  content text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);
