ALTER TABLE agents ADD COLUMN retired_at timestamptz;
ALTER TABLE model_connections ADD COLUMN retired_at timestamptz;

WITH ranked AS (
  SELECT a.id,a.model_connection_id,
    row_number() OVER(PARTITION BY a.model_connection_id
      ORDER BY (a.id='00000000-0000-0000-0000-000000000002') DESC,a.created_at,a.id) AS position
  FROM agents a
), copies AS (
  SELECT id AS agent_id,model_connection_id,gen_random_uuid() AS model_id
  FROM ranked WHERE position>1
), inserted AS (
  INSERT INTO model_connections(id,owner_id,name,model_revision)
  SELECT c.model_id,m.owner_id,m.name,m.model_revision
  FROM copies c JOIN model_connections m ON m.id=c.model_connection_id
  RETURNING id
)
UPDATE agents a SET model_connection_id=i.id,revision=a.revision+1,updated_at=now()
FROM copies c JOIN inserted i ON i.id=c.model_id WHERE a.id=c.agent_id;

WITH active_task AS (
  SELECT c.owner_id,r.agent_id FROM runs r
  JOIN conversations c ON c.id=r.conversation_id
  JOIN agents a ON a.id=r.agent_id AND a.owner_id=c.owner_id
  JOIN model_connections m ON m.id=a.model_connection_id AND m.owner_id=c.owner_id
  WHERE r.origin='user' AND r.status IN ('queued','running','paused') AND m.model_revision>0
), selection AS (
  SELECT p.owner_id, (
    SELECT a.id FROM agents a JOIN model_connections m ON m.id=a.model_connection_id
    WHERE a.owner_id=p.owner_id AND m.owner_id=p.owner_id AND m.model_revision>0
    ORDER BY (a.id IS DISTINCT FROM COALESCE(t.agent_id,p.default_agent_id)),a.created_at,a.id LIMIT 1
  ) AS agent_id
  FROM devices p LEFT JOIN active_task t ON t.owner_id=p.owner_id WHERE p.id='phone-local-01'
)
UPDATE devices d SET default_agent_id=s.agent_id,revision=d.revision+1,updated_at=now()
FROM selection s WHERE d.owner_id=s.owner_id AND d.runtime_state<>'archived'
AND d.default_agent_id IS DISTINCT FROM s.agent_id;

INSERT INTO agent_device_bindings(agent_id,device_id)
SELECT default_agent_id,id FROM devices
WHERE runtime_state<>'archived' AND default_agent_id IS NOT NULL
ON CONFLICT DO NOTHING;

DELETE FROM agent_device_bindings b USING devices d
WHERE b.device_id=d.id AND d.runtime_state<>'archived'
AND b.agent_id IS DISTINCT FROM d.default_agent_id;

WITH stopped AS (
  UPDATE runs r SET status='canceled',error='已切换模型',finished_at=now(),next_wake_at=NULL
  FROM conversations c, devices p
  WHERE r.conversation_id=c.id AND p.id='phone-local-01' AND p.owner_id=c.owner_id
  AND r.status IN ('queued','running','paused') AND r.origin<>'user'
  AND r.agent_id IS DISTINCT FROM p.default_agent_id RETURNING r.id,r.finished_at
), trace AS (
  UPDATE harness_events e SET status='canceled',finished_at=s.finished_at FROM stopped s WHERE e.id=s.id
)
UPDATE phone_notifications n SET status='pending-summary'
FROM stopped s WHERE n.run_id=s.id AND n.status IN ('assigned','attending');
