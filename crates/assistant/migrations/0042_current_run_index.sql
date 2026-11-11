CREATE INDEX runs_current_conversation ON runs (
    conversation_id,
    (status IN ('queued','running','paused')) DESC,
    GREATEST(input_at,finished_at,created_at) DESC,
    id DESC
);
