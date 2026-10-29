-- Immutable server-side model revisions contain credentials outside the database.
-- Zero represents a pre-migration run and is pinned before its next model call.
ALTER TABLE runs ADD COLUMN model_revision bigint NOT NULL DEFAULT 0;
