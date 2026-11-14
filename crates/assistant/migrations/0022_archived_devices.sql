-- Retire a device without deleting its disk, conversations, observations or runs.
ALTER TABLE devices DROP CONSTRAINT devices_runtime_state_check;
ALTER TABLE devices ADD CONSTRAINT devices_runtime_state_check
  CHECK(runtime_state IN ('managed','registered','archived'));
