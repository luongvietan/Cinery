CREATE TABLE provider_audit_events (
  id TEXT PRIMARY KEY,
  execution_id TEXT,
  workflow_run_id TEXT NOT NULL,
  event_type TEXT NOT NULL,
  payload_json TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY (execution_id) REFERENCES workflow_step_executions(id),
  FOREIGN KEY (workflow_run_id) REFERENCES workflow_runs(id)
);

CREATE INDEX idx_provider_audit_events_run
  ON provider_audit_events(workflow_run_id, created_at);
