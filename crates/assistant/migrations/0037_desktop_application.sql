-- Old observations remain identity-unknown; titles are private, not application IDs.
ALTER TABLE desktop_observations ADD COLUMN application_id text NOT NULL DEFAULT '';
CREATE INDEX app_experiences_approved_lookup ON app_experiences(app_id,title,created_at DESC)
WHERE approved AND superseded_at IS NULL;
