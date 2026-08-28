-- Performance indexes derived from actual query paths in P0-P8 features.
-- Only composite indexes that existing queries filter/sort on are added.

-- workflow_steps: get_run orders by step_index; recovery filters by run.
CREATE INDEX idx_workflow_steps_run
  ON workflow_steps(workflow_run_id, step_index);

-- workflow_events: get_run orders by sequence.
CREATE INDEX idx_workflow_events_run
  ON workflow_events(workflow_run_id, sequence);

-- qa_runs: cancel/fail-for-workflow and diagnostics lookups by run.
CREATE INDEX idx_qa_runs_workflow
  ON qa_runs(workflow_run_id, created_at);

-- artifact_lineage: provenance traverses backwards from artifact.
CREATE INDEX idx_artifact_lineage_artifact
  ON artifact_lineage(artifact_id);

-- workflow_approvals: runtime looks up approval per run+step.
CREATE INDEX idx_workflow_approvals_run_step
  ON workflow_approvals(workflow_run_id, step_definition_id);

-- workflow_runs: active-job surfaces filter project+status and sort by recency.
CREATE INDEX idx_workflow_runs_project_status
  ON workflow_runs(project_id, status, updated_at DESC);

-- asset_versions: canonical-slot checks filter asset+status.
CREATE INDEX idx_asset_versions_asset_status
  ON asset_versions(asset_id, status);

-- qa_runs: per-version history within a project.
CREATE INDEX idx_qa_runs_project_version
  ON qa_runs(project_id, asset_version_id, created_at DESC);
