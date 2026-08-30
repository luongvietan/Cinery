-- 0021_shot_video_pin.sql
--
-- P10.0: the shot-level exact video reference. A shot may pin one exact,
-- immutable video AssetVersion (mirroring keyframe_asset_version_id).
--
-- Semantics (enforced in cinema/service.rs, identical to the keyframe pin):
--   * nullable -- a shot has no video until the user pins one;
--   * pins an EXACT version id -- promoting a newer video version never
--     mutates this column (no canonical drift);
--   * validated to be an in-project canonical version of an asset with
--     type = 'video' at pin time;
--   * changing it is an explicit user action (set_shot_video command).
--
-- SQLite cannot add a FOREIGN KEY with ALTER TABLE, and the column belongs
-- with the table's other version pins, so scene_shots is rebuilt. No table
-- references scene_shots (verified: only self-contained indexes), so the
-- rebuild cannot orphan inbound foreign keys; the table's own outbound FKs
-- (scene_id -> world_scenes, keyframe/pin -> asset_versions) are re-declared
-- identically and the copied rows already satisfy them. The migration is
-- still flagged as an FK rebuild in db/migrations.rs so the runner verifies
-- `PRAGMA foreign_key_check` before committing.

DROP INDEX IF EXISTS idx_scene_shots_scene;

CREATE TABLE scene_shots_migrated (
    id TEXT PRIMARY KEY,
    scene_id TEXT NOT NULL,
    ordering INTEGER NOT NULL CHECK (ordering >= 0),
    duration_seconds REAL NOT NULL CHECK (duration_seconds > 0 AND duration_seconds <= 30),
    keyframe_asset_version_id TEXT,
    generated_video_asset_version_id TEXT,
    intent TEXT NOT NULL CHECK (length(trim(intent)) BETWEEN 1 AND 240),
    action TEXT,
    camera TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (scene_id) REFERENCES world_scenes(id) ON DELETE CASCADE,
    FOREIGN KEY (keyframe_asset_version_id) REFERENCES asset_versions(id),
    FOREIGN KEY (generated_video_asset_version_id) REFERENCES asset_versions(id),
    UNIQUE(scene_id, ordering)
);

INSERT INTO scene_shots_migrated (
    id, scene_id, ordering, duration_seconds, keyframe_asset_version_id,
    intent, action, camera, created_at, updated_at
)
SELECT
    id, scene_id, ordering, duration_seconds, keyframe_asset_version_id,
    intent, action, camera, created_at, updated_at
FROM scene_shots;

DROP TABLE scene_shots;
ALTER TABLE scene_shots_migrated RENAME TO scene_shots;

CREATE INDEX idx_scene_shots_scene ON scene_shots(scene_id, ordering);
