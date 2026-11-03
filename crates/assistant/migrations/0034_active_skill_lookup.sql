-- Shared by task-local experience recall and case-insensitive skill reads.
-- Keep removed skills and user edits intact; this changes only the access path.
CREATE INDEX runtime_skills_active_name ON runtime_skills(lower(name))
WHERE deleted_at IS NULL;
