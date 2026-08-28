CREATE TABLE qa_repairs (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  source_asset_id TEXT NOT NULL,
  source_asset_version_id TEXT NOT NULL,
  child_asset_version_id TEXT NOT NULL UNIQUE,
  source_qa_run_id TEXT NOT NULL,
  workflow_run_id TEXT NOT NULL UNIQUE,
  failed_check_ids_json TEXT NOT NULL CHECK (json_valid(failed_check_ids_json)),
  repair_plan_json TEXT NOT NULL CHECK (json_valid(repair_plan_json)),
  compiled_request_json TEXT NOT NULL CHECK (json_valid(compiled_request_json)),
  provider_id TEXT NOT NULL,
  adapter_version INTEGER NOT NULL,
  model_id TEXT NOT NULL,
  provider_job_id TEXT NOT NULL,
  reference_asset_version_ids_json TEXT NOT NULL CHECK (json_valid(reference_asset_version_ids_json)),
  child_qa_run_id TEXT,
  auto_qa_workflow_run_id TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
  FOREIGN KEY (source_asset_id) REFERENCES assets(id) ON DELETE RESTRICT,
  FOREIGN KEY (source_asset_version_id) REFERENCES asset_versions(id) ON DELETE RESTRICT,
  FOREIGN KEY (child_asset_version_id) REFERENCES asset_versions(id) ON DELETE RESTRICT,
  FOREIGN KEY (source_qa_run_id) REFERENCES qa_runs(id) ON DELETE RESTRICT,
  FOREIGN KEY (workflow_run_id) REFERENCES workflow_runs(id) ON DELETE RESTRICT,
  FOREIGN KEY (child_qa_run_id) REFERENCES qa_runs(id) ON DELETE RESTRICT,
  FOREIGN KEY (auto_qa_workflow_run_id) REFERENCES workflow_runs(id) ON DELETE RESTRICT
);

CREATE INDEX idx_qa_repairs_source_version
  ON qa_repairs(source_asset_version_id, created_at DESC);

