-- Preserve every message, attachment reference and run in one continuous record.
DO $$
DECLARE canonical uuid;
BEGIN
  SELECT id INTO canonical FROM conversations WHERE owner_id='local-user' ORDER BY created_at,id LIMIT 1;
  IF canonical IS NULL THEN
    canonical := '00000000-0000-7000-8000-000000000001';
    INSERT INTO conversations(id) VALUES(canonical);
  END IF;
  UPDATE messages SET conversation_id=canonical WHERE conversation_id<>canonical;
  UPDATE runs SET conversation_id=canonical WHERE conversation_id<>canonical;
  DELETE FROM conversations WHERE id<>canonical;
END $$;
ALTER TABLE conversations ADD COLUMN singleton boolean NOT NULL DEFAULT true CHECK(singleton);
ALTER TABLE conversations ADD CONSTRAINT one_continuous_conversation UNIQUE(singleton);
