-- 0024_shot_video_review_states.sql
--
-- P10.4 adds human review state for shot video candidates. Review and
-- canonicality are orthogonal dimensions:
--
--   * canonical selection remains exactly where it was — the nullable,
--     exact-version pin `scene_shots.generated_video_asset_version_id`
--     plus `assets.canonical_version_id` (P10.2 promotion semantics);
--   * this table only records whether a human has rejected (or restored)
--     a candidate, so rejected candidates stay visible-but-de-emphasized,
--     are never promotable, and never delete artifacts or QA history.
--
-- One row may exist per scene video asset version. Absence of a row means
-- `active` (the default review state), so generation produces no review
-- rows and QA writes none either. Rejection is reversible: the row is
-- deleted on restore, returning the candidate to Active.

CREATE TABLE shot_video_review_states (
  asset_version_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('active', 'rejected')),
  reason TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (asset_version_id) REFERENCES asset_versions(id) ON DELETE CASCADE,
  FOREIGN KEY (project_id) REFERENCES projects(id)
);

CREATE INDEX idx_shot_video_review_states_project
  ON shot_video_review_states(project_id, state);
