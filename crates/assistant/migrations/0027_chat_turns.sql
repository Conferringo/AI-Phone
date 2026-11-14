-- Stable accepted order and receipts survive interrupted responses and retries.
CREATE SEQUENCE messages_ordinal_seq;
ALTER TABLE messages ADD COLUMN ordinal bigint;
WITH ordered AS (
  SELECT id,row_number() OVER (ORDER BY created_at,id) AS position FROM messages
)
UPDATE messages SET ordinal=ordered.position FROM ordered WHERE messages.id=ordered.id;
SELECT setval('messages_ordinal_seq',COALESCE((SELECT max(ordinal) FROM messages),0)+1,false);
ALTER TABLE messages ALTER COLUMN ordinal SET DEFAULT nextval('messages_ordinal_seq');
ALTER TABLE messages ALTER COLUMN ordinal SET NOT NULL;
ALTER SEQUENCE messages_ordinal_seq OWNED BY messages.ordinal;
CREATE UNIQUE INDEX messages_ordinal ON messages(ordinal);
CREATE INDEX messages_conversation_order ON messages(conversation_id,ordinal);
ALTER TABLE messages ADD COLUMN request_id uuid;
CREATE UNIQUE INDEX messages_request ON messages(request_id) WHERE request_id IS NOT NULL;
ALTER TABLE messages ADD COLUMN run_id uuid REFERENCES runs(id) ON DELETE SET NULL;
UPDATE messages m SET run_id=r.id FROM runs r WHERE r.assistant_message_id=m.id;
UPDATE messages m SET request_id=i.request_id FROM run_inputs i WHERE i.message_id=m.id;
ALTER TABLE runs ADD COLUMN input_at timestamptz;
