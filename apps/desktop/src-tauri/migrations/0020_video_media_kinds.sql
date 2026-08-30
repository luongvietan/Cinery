-- 0020_video_media_kinds.sql
--
-- The P10.0 video pipeline persists 'video' media kinds and 'video/mp4'
-- MIME types, but three tables shipped with image-only CHECK constraints:
--
--   asset_versions.mime_type            (migrations/0002, 0002:45-47)
--   generation_result_sets.media_kind   (migrations/0008, 0008:7)
--   generated_artifacts.media_kind      (migrations/0008, 0008:22)
--   generated_artifacts.mime_type       (migrations/0008, 0008:23)
--
-- SQLite cannot ALTER a CHECK constraint in place, so each affected table
-- is rebuilt following SQLite's documented 12-step procedure (see
-- https://sqlite.org/lang_altertable.html section 7). This migration is
-- flagged `rebuilds_foreign_key_tables` in db/migrations.rs: the runner
-- disables `PRAGMA foreign_keys` before the transaction (the pragma is a
-- no-op inside one), runs the rebuild transactionally, verifies
-- `PRAGMA foreign_key_check` inside the transaction, and re-enables
-- enforcement afterwards.
--
-- Constraints are widened to an explicit supported set -- NOT free text:
-- media kinds remain 'image' | 'video'; MIME types remain the three image
-- formats plus 'video/mp4' (the only video container the provider
-- ingestion currently validates, via the ISO-BMFF `ftyp` box check in
-- assets/import.rs and generation/storage.rs).
--
-- Data preservation: every row, id, unique constraint, and index is
-- carried over unchanged. Referencing tables (artifact_lineage,
-- generated_artifact_sources, artifact_promotions, qa_runs, qa_repairs,
-- scene pins, the legacy P8 tables) are untouched; because
-- `foreign_keys` is OFF during the rebuild, the DROP/RENAME dance cannot
-- trip their references, and `foreign_key_check` proves integrity before
-- commit.

-- ---------------------------------------------------------------------------
-- 1) asset_versions: widen mime_type to include video/mp4.
-- ---------------------------------------------------------------------------

DROP INDEX IF EXISTS idx_asset_versions_asset_id;
DROP INDEX IF EXISTS idx_asset_versions_asset_status;

CREATE TABLE asset_versions_migrated (
  id TEXT PRIMARY KEY,
  asset_id TEXT NOT NULL,
  version_number INTEGER NOT NULL CHECK (version_number > 0),
  status TEXT NOT NULL CHECK (
    status IN (
      'draft',
      'generated',
      'candidate',
      'qa_failed',
      'repairing',
      'approved',
      'canonical',
      'superseded'
    )
  ),
  file_path TEXT NOT NULL,
  thumbnail_path TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  original_filename TEXT NOT NULL,
  mime_type TEXT NOT NULL CHECK (
    mime_type IN ('image/png', 'image/jpeg', 'image/webp', 'video/mp4')
  ),
  byte_size INTEGER NOT NULL CHECK (byte_size >= 0),
  parent_version_id TEXT,
  created_at TEXT NOT NULL,
  width INTEGER CHECK (width IS NULL OR width > 0),
  height INTEGER CHECK (height IS NULL OR height > 0),
  FOREIGN KEY (asset_id) REFERENCES assets(id),
  FOREIGN KEY (parent_version_id) REFERENCES asset_versions(id),
  UNIQUE(asset_id, version_number),
  UNIQUE(asset_id, sha256)
);

INSERT INTO asset_versions_migrated (
  id, asset_id, version_number, status, file_path, thumbnail_path, sha256,
  original_filename, mime_type, byte_size, parent_version_id, created_at,
  width, height
)
SELECT
  id, asset_id, version_number, status, file_path, thumbnail_path, sha256,
  original_filename, mime_type, byte_size, parent_version_id, created_at,
  width, height
FROM asset_versions;

DROP TABLE asset_versions;
ALTER TABLE asset_versions_migrated RENAME TO asset_versions;

CREATE INDEX idx_asset_versions_asset_id ON asset_versions(asset_id);
CREATE INDEX idx_asset_versions_asset_status ON asset_versions(asset_id, status);

-- ---------------------------------------------------------------------------
-- 2) generation_result_sets: widen media_kind to image | video.
-- ---------------------------------------------------------------------------

DROP INDEX IF EXISTS idx_generation_result_sets_project;

CREATE TABLE generation_result_sets_migrated (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  workflow_run_id TEXT NOT NULL,
  workflow_step_key TEXT NOT NULL,
  provider_attempt_id TEXT NOT NULL UNIQUE,
  media_kind TEXT NOT NULL CHECK (media_kind IN ('image', 'video')),
  requested_output_count INTEGER NOT NULL CHECK (requested_output_count > 0),
  created_at TEXT NOT NULL,
  FOREIGN KEY (project_id) REFERENCES projects(id),
  FOREIGN KEY (workflow_run_id) REFERENCES workflow_runs(id),
  FOREIGN KEY (provider_attempt_id) REFERENCES workflow_step_executions(id)
);

INSERT INTO generation_result_sets_migrated (
  id, project_id, workflow_run_id, workflow_step_key, provider_attempt_id,
  media_kind, requested_output_count, created_at
)
SELECT
  id, project_id, workflow_run_id, workflow_step_key, provider_attempt_id,
  media_kind, requested_output_count, created_at
FROM generation_result_sets;

DROP TABLE generation_result_sets;
ALTER TABLE generation_result_sets_migrated RENAME TO generation_result_sets;

CREATE INDEX idx_generation_result_sets_project
  ON generation_result_sets(project_id, created_at DESC);

-- ---------------------------------------------------------------------------
-- 3) generated_artifacts: widen media_kind and mime_type.
-- ---------------------------------------------------------------------------

DROP INDEX IF EXISTS idx_generated_artifacts_result_set;

CREATE TABLE generated_artifacts_migrated (
  id TEXT PRIMARY KEY,
  result_set_id TEXT NOT NULL,
  ordinal INTEGER NOT NULL CHECK (ordinal > 0),
  media_kind TEXT NOT NULL CHECK (media_kind IN ('image', 'video')),
  mime_type TEXT NOT NULL CHECK (
    mime_type IN ('image/png', 'image/jpeg', 'image/webp', 'video/mp4')
  ),
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

INSERT INTO generated_artifacts_migrated (
  id, result_set_id, ordinal, media_kind, mime_type, width, height,
  byte_size, sha256, storage_path, capture_status, capture_error_code,
  created_at
)
SELECT
  id, result_set_id, ordinal, media_kind, mime_type, width, height,
  byte_size, sha256, storage_path, capture_status, capture_error_code,
  created_at
FROM generated_artifacts;

DROP TABLE generated_artifacts;
ALTER TABLE generated_artifacts_migrated RENAME TO generated_artifacts;

CREATE INDEX idx_generated_artifacts_result_set
  ON generated_artifacts(result_set_id, ordinal);
