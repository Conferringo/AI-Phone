-- Explicitly enabled exploration is independent of an old paused owner task.
-- Owner steering still cancels background lanes before selecting its user run.
DROP INDEX runs_one_active_per_conversation;
CREATE UNIQUE INDEX runs_one_active_per_conversation ON runs(conversation_id)
 WHERE status IN ('queued','running','paused')
 AND origin NOT IN ('notification','exploration','desktop-exploration');
CREATE UNIQUE INDEX runs_one_exploration_per_conversation ON runs(conversation_id)
 WHERE status IN ('queued','running','paused')
 AND origin IN ('exploration','desktop-exploration');
