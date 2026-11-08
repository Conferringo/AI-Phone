-- Existing IDs/messages/runs remain on the original phone.
ALTER TABLE conversations DROP CONSTRAINT one_continuous_conversation;
ALTER TABLE conversations DROP COLUMN singleton;
ALTER TABLE conversations ADD COLUMN device_id text NOT NULL DEFAULT 'phone-local-01';
CREATE UNIQUE INDEX conversation_per_device ON conversations(owner_id,device_id);
ALTER TABLE memories ADD COLUMN agent_id uuid REFERENCES agents(id);
UPDATE memories SET agent_id='00000000-0000-0000-0000-000000000002'
WHERE EXISTS(SELECT 1 FROM agents WHERE id='00000000-0000-0000-0000-000000000002');
CREATE INDEX memories_agent ON memories(agent_id,created_at);
