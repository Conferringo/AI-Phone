ALTER TABLE device_messages ADD COLUMN kafka_pending boolean NOT NULL DEFAULT false;
CREATE INDEX device_messages_kafka_pending ON device_messages(created_at,id) WHERE kafka_pending;
