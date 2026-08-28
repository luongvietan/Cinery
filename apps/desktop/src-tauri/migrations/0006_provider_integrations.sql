CREATE TABLE provider_configurations (
  provider_id TEXT PRIMARY KEY,
  enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
  credential_reference TEXT,
  default_model TEXT,
  endpoint TEXT,
  request_timeout_seconds INTEGER NOT NULL DEFAULT 60,
  polling_interval_seconds INTEGER NOT NULL DEFAULT 3,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE workflow_step_executions (
  id TEXT PRIMARY KEY,
  workflow_run_id TEXT NOT NULL,
  step_definition_id TEXT NOT NULL,
  attempt_number INTEGER NOT NULL CHECK (attempt_number > 0),
  compiled_request_id TEXT NOT NULL,
  provider_id TEXT NOT NULL,
  model_id TEXT NOT NULL,
  adapter_version INTEGER NOT NULL,
  idempotency_key TEXT NOT NULL UNIQUE,
  status TEXT NOT NULL CHECK (status IN (
    'queued', 'submitted', 'running', 'succeeded', 'failed',
    'cancellation_requested', 'cancelled', 'unknown'
  )),
  provider_job_id TEXT,
  normalized_error_json TEXT,
  artifact_ids_json TEXT NOT NULL DEFAULT '[]',
  started_at TEXT NOT NULL,
  completed_at TEXT,
  FOREIGN KEY (workflow_run_id) REFERENCES workflow_runs(id)
);

CREATE UNIQUE INDEX idx_execution_attempt_identity
  ON workflow_step_executions(workflow_run_id, step_definition_id, attempt_number);

CREATE INDEX idx_execution_active_jobs
  ON workflow_step_executions(status, provider_id, provider_job_id);

CREATE TABLE provider_jobs (
  id TEXT PRIMARY KEY,
  execution_id TEXT NOT NULL UNIQUE,
  provider_id TEXT NOT NULL,
  provider_job_id TEXT NOT NULL,
  status TEXT NOT NULL,
  submitted_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (execution_id) REFERENCES workflow_step_executions(id)
);

CREATE UNIQUE INDEX idx_provider_jobs_remote_identity
  ON provider_jobs(provider_id, provider_job_id);
