CREATE TABLE generation_result_sets (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  workflow_run_id TEXT NOT NULL,
  workflow_step_key TEXT NOT NULL,
  provider_attempt_id TEXT NOT NULL UNIQUE,
  media_kind TEXT NOT NULL CHECK (media_kind = 'image'),
  requested_output_count INTEGER NOT NULL CHECK (requested_output_count > 0),
  created_at TEXT NOT NULL,
  FOREIGN KEY (project_id) REFERENCES projects(id),
  FOREIGN KEY (workflow_run_id) REFERENCES workflow_runs(id),
  FOREIGN KEY (provider_attempt_id) REFERENCES workflow_step_executions(id)
);

CREATE INDEX idx_generation_result_sets_project
  ON generation_result_sets(project_id, created_at DESC);

CREATE TABLE generated_artifacts (
  id TEXT PRIMARY KEY,
  result_set_id TEXT NOT NULL,
  ordinal INTEGER NOT NULL CHECK (ordinal > 0),
  media_kind TEXT NOT NULL CHECK (media_kind = 'image'),
  mime_type TEXT NOT NULL CHECK (mime_type IN ('image/png', 'image/jpeg', 'image/webp')),
  width INTEGER CHECK (width IS NULL OR width > 0),
  height INTEGER CHECK (height IS NULL OR height > 0),
  byte_size INTEGER NOT NULL CHECK (byte_size >= 0),
  sha256 TEXT NOT NULL CHECK (length(sha256) = 64),
  storage_path TEXT NOT NULL UNIQUE,
  capture_status TEXT NOT NULL CHECK (capture_status IN ('materializing', 'available', 'failed')),
  capture_error_code TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY (result_set_id) REFERENCES generation_result_sets(id),
  UNIQUE(result_set_id, ordinal)
);

CREATE INDEX idx_generated_artifacts_result_set
  ON generated_artifacts(result_set_id, ordinal);

CREATE TABLE generated_artifact_sources (
  artifact_id TEXT NOT NULL,
  asset_version_id TEXT NOT NULL,
  role TEXT NOT NULL,
  ordinal INTEGER NOT NULL CHECK (ordinal > 0),
  FOREIGN KEY (artifact_id) REFERENCES generated_artifacts(id),
  FOREIGN KEY (asset_version_id) REFERENCES asset_versions(id),
  PRIMARY KEY (artifact_id, ordinal),
  UNIQUE(artifact_id, asset_version_id, role)
);
