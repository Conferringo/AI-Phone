-- One attempt owns each device until its result is durably finalized.
-- No expiry: a slow model must not let another run steal its device.
CREATE TABLE device_execution_leases (
  device_id text PRIMARY KEY,
  run_id uuid NOT NULL REFERENCES runs(id),
  acquired_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX device_execution_leases_run ON device_execution_leases(run_id);
CREATE INDEX runs_running_device ON runs(device_id) WHERE status='running';
