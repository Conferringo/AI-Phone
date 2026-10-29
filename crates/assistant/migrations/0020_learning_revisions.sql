ALTER TABLE app_experiences ADD COLUMN superseded_at timestamptz;
WITH ranked AS (
 SELECT id,row_number() OVER(PARTITION BY run_id,app_id,title ORDER BY created_at DESC,id DESC) n FROM app_experiences
)
UPDATE app_experiences SET superseded_at=now() WHERE id IN(SELECT id FROM ranked WHERE n>1);
CREATE UNIQUE INDEX app_experience_current_draft ON app_experiences(run_id,app_id,title) WHERE superseded_at IS NULL;
