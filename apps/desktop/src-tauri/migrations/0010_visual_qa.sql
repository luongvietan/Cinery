CREATE TABLE qa_runs (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  asset_id TEXT NOT NULL,
  asset_version_id TEXT NOT NULL,
  workflow_run_id TEXT,
  status TEXT NOT NULL CHECK (status IN (
    'queued', 'running', 'succeeded', 'failed', 'cancelled'
  )),
  overall_status TEXT CHECK (overall_status IN (
    'pass', 'fail', 'needs_review'
  )),
  adapter_id TEXT,
  adapter_version TEXT,
  model_id TEXT,
  execution_location TEXT NOT NULL CHECK (
    execution_location = 'local' OR execution_location LIKE 'cloud:%'
  ),
  check_plan_json TEXT NOT NULL,
  context_snapshot_json TEXT NOT NULL,
  raw_response_metadata_json TEXT,
  error_code TEXT,
  error_message TEXT,
  created_at TEXT NOT NULL,
  started_at TEXT,
  completed_at TEXT,
  FOREIGN KEY (project_id) REFERENCES projects(id),
  FOREIGN KEY (asset_id) REFERENCES assets(id),
  FOREIGN KEY (asset_version_id) REFERENCES asset_versions(id),
  FOREIGN KEY (workflow_run_id) REFERENCES workflow_runs(id)
);

CREATE INDEX idx_qa_runs_asset_version
  ON qa_runs(asset_version_id, created_at DESC);

CREATE INDEX idx_qa_runs_project
  ON qa_runs(project_id, created_at DESC);

CREATE TABLE qa_checks (
  id TEXT PRIMARY KEY,
  qa_run_id TEXT NOT NULL,
  check_id TEXT NOT NULL,
  check_type TEXT NOT NULL CHECK (check_type IN (
    'identity_similarity',
    'permanent_visual_lock',
    'hair_consistency',
    'skin_register',
    'outfit_piece',
    'accessory_placement',
    'required_element',
    'forbidden_element',
    'background_requirement',
    'composition_requirement',
    'watermark',
    'unexpected_artifact'
  )),
  source TEXT NOT NULL CHECK (source IN (
    'visual_lock', 'canonical_reference', 'operation_expectation', 'artifact_detection'
  )),
  requirement_json TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN (
    'pass', 'fail', 'uncertain', 'not_applicable'
  )),
  confidence REAL CHECK (confidence IS NULL OR (confidence >= 0.0 AND confidence <= 1.0)),
  observed TEXT NOT NULL,
  reason TEXT NOT NULL,
  repair_hint TEXT,
  review_status TEXT NOT NULL DEFAULT 'unreviewed' CHECK (review_status IN (
    'unreviewed', 'confirmed', 'overridden_pass', 'overridden_fail'
  )),
  review_note TEXT,
  reviewed_at TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY (qa_run_id) REFERENCES qa_runs(id),
  UNIQUE(qa_run_id, check_id)
);

CREATE INDEX idx_qa_checks_run_status
  ON qa_checks(qa_run_id, status);
