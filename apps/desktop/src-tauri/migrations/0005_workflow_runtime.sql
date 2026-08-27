CREATE TABLE workflow_runs (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  skill_id TEXT NOT NULL,
  skill_version TEXT NOT NULL,
  operation_id TEXT NOT NULL,
  status TEXT NOT NULL CHECK (
    status IN (
      'created',
      'running',
      'waiting_for_approval',
      'ready_for_execution',
      'completed',
      'rejected',
      'cancelled',
      'failed'
    )
  ),
  input_json TEXT NOT NULL,
  prerequisite_report_json TEXT,
  context_snapshot_json TEXT,
  current_step_index INTEGER NOT NULL DEFAULT 0,
  failure_code TEXT,
  failure_message TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  completed_at TEXT,
  FOREIGN KEY (project_id) REFERENCES projects(id)
);

CREATE INDEX idx_workflow_runs_project
  ON workflow_runs(project_id, created_at DESC);

CREATE TABLE workflow_steps (
  id TEXT PRIMARY KEY,
  workflow_run_id TEXT NOT NULL,
  step_definition_id TEXT NOT NULL,
  step_index INTEGER NOT NULL,
  step_type TEXT NOT NULL CHECK (
    step_type IN (
      'validate_input',
      'resolve_context',
      'compile_request',
      'approval',
      'execute',
      'complete'
    )
  ),
  status TEXT NOT NULL CHECK (
    status IN (
      'pending',
      'running',
      'waiting',
      'completed',
      'skipped',
      'failed'
    )
  ),
  input_json TEXT,
  output_json TEXT,
  started_at TEXT,
  completed_at TEXT,
  FOREIGN KEY (workflow_run_id) REFERENCES workflow_runs(id),
  UNIQUE(workflow_run_id, step_index),
  UNIQUE(workflow_run_id, step_definition_id)
);

CREATE TABLE workflow_events (
  id TEXT PRIMARY KEY,
  workflow_run_id TEXT NOT NULL,
  sequence INTEGER NOT NULL CHECK (sequence > 0),
  type TEXT NOT NULL CHECK (
    type IN (
      'run_created',
      'run_started',
      'step_started',
      'step_completed',
      'approval_requested',
      'approval_granted',
      'approval_rejected',
      'execution_started',
      'execution_completed',
      'run_completed',
      'run_cancelled',
      'run_failed'
    )
  ),
  step_definition_id TEXT,
  payload_json TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY (workflow_run_id) REFERENCES workflow_runs(id),
  UNIQUE(workflow_run_id, sequence)
);

CREATE TABLE workflow_approvals (
  id TEXT PRIMARY KEY,
  workflow_run_id TEXT NOT NULL,
  step_definition_id TEXT NOT NULL,
  decision TEXT NOT NULL CHECK (
    decision IN ('approved', 'rejected')
  ),
  artifact_json TEXT NOT NULL,
  note TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY (workflow_run_id) REFERENCES workflow_runs(id),
  UNIQUE(workflow_run_id, step_definition_id)
);
