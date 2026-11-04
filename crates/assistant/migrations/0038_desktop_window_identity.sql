-- Bind semantic control inspection to the actual observed foreground window.
ALTER TABLE desktop_observations
  ADD COLUMN window_id bigint NOT NULL DEFAULT 0,
  ADD COLUMN process_id bigint NOT NULL DEFAULT 0;
