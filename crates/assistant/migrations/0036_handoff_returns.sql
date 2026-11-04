-- The originating run consumes a receiving run's terminal receipt exactly once.
-- Keep the existing message/run identities; do not enqueue a second device task.
ALTER TABLE device_messages ADD COLUMN return_delivered_at timestamptz;
CREATE INDEX device_messages_unread_returns ON device_messages(source_run_id,created_at,id)
  WHERE source_run_id IS NOT NULL AND return_delivered_at IS NULL
    AND status IN ('completed','failed','canceled');
