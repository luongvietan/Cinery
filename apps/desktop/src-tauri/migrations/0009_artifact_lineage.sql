CREATE TABLE artifact_lineage (
  artifact_id TEXT PRIMARY KEY,
  workflow_run_id TEXT NOT NULL,
  workflow_step_key TEXT NOT NULL,
  workflow_definition_id TEXT NOT NULL,
  workflow_version TEXT NOT NULL,
  skill_id TEXT NOT NULL,
  skill_version TEXT NOT NULL,
  compiled_execution_artifact_id TEXT NOT NULL,
  compiled_request_sha256 TEXT NOT NULL CHECK (length(compiled_request_sha256) = 64),
  canon_snapshot_id TEXT,
  canon_snapshot_sha256 TEXT CHECK (canon_snapshot_sha256 IS NULL OR length(canon_snapshot_sha256) = 64),
  provider_attempt_id TEXT NOT NULL,
  provider_id TEXT NOT NULL,
  model_id TEXT NOT NULL,
  created_at TEXT NOT NULL,
  FOREIGN KEY (artifact_id) REFERENCES generated_artifacts(id),
  FOREIGN KEY (workflow_run_id) REFERENCES workflow_runs(id),
  FOREIGN KEY (provider_attempt_id) REFERENCES workflow_step_executions(id)
);

CREATE TABLE artifact_promotions (
  id TEXT PRIMARY KEY,
  artifact_id TEXT NOT NULL UNIQUE,
  asset_id TEXT NOT NULL,
  asset_version_id TEXT NOT NULL UNIQUE,
  set_canonical INTEGER NOT NULL CHECK (set_canonical IN (0, 1)),
  created_at TEXT NOT NULL,
  FOREIGN KEY (artifact_id) REFERENCES generated_artifacts(id),
  FOREIGN KEY (asset_id) REFERENCES assets(id),
  FOREIGN KEY (asset_version_id) REFERENCES asset_versions(id)
);

CREATE INDEX idx_artifact_lineage_provider_attempt
  ON artifact_lineage(provider_attempt_id);
