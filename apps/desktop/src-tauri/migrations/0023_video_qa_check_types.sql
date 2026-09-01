-- 0023_video_qa_check_types.sql
--
-- P10.3 reuses the P6 qa_runs/qa_checks history and review tables. The run
-- table already owns one exact asset_version_id, but qa_checks shipped with
-- an image-only check_type constraint. SQLite cannot widen a CHECK constraint
-- in place, so rebuild only qa_checks and preserve every column, row,
-- constraint, unique key, foreign key, and index.

DROP INDEX IF EXISTS idx_qa_checks_run_status;

CREATE TABLE qa_checks_migrated (
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
    'unexpected_artifact',
    'video_integrity',
    'start_frame_continuity',
    'identity_temporal_consistency',
    'reference_temporal_consistency',
    'motion_adherence',
    'camera_motion_adherence',
    'temporal_coherence',
    'unexpected_cut',
    'flicker',
    'deformation_or_warping'
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

INSERT INTO qa_checks_migrated (
  id, qa_run_id, check_id, check_type, source, requirement_json, status,
  confidence, observed, reason, repair_hint, review_status, review_note,
  reviewed_at, created_at
)
SELECT
  id, qa_run_id, check_id, check_type, source, requirement_json, status,
  confidence, observed, reason, repair_hint, review_status, review_note,
  reviewed_at, created_at
FROM qa_checks;

DROP TABLE qa_checks;
ALTER TABLE qa_checks_migrated RENAME TO qa_checks;

CREATE INDEX idx_qa_checks_run_status
  ON qa_checks(qa_run_id, status);
