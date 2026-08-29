-- Unified Scene domain (P9 integration stabilization).
--
-- `world_scenes` (migration 0016) is the authoritative Scene aggregate. The
-- legacy P8 cinema subsystem stored its own parallel scene rows in `scenes`
-- with `shots` and `cinema_compilations` keyed to them. This migration gives
-- shots and compilations a home on the authoritative aggregate and moves any
-- legacy rows across deterministically. The legacy tables are retained
-- read-only: no row is destroyed.

CREATE TABLE scene_shots (
    id TEXT PRIMARY KEY,
    scene_id TEXT NOT NULL,
    ordering INTEGER NOT NULL CHECK (ordering >= 0),
    duration_seconds REAL NOT NULL CHECK (duration_seconds > 0 AND duration_seconds <= 30),
    keyframe_asset_version_id TEXT,
    intent TEXT NOT NULL CHECK (length(trim(intent)) BETWEEN 1 AND 240),
    action TEXT,
    camera TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (scene_id) REFERENCES world_scenes(id) ON DELETE CASCADE,
    FOREIGN KEY (keyframe_asset_version_id) REFERENCES asset_versions(id),
    UNIQUE(scene_id, ordering)
);
CREATE INDEX idx_scene_shots_scene ON scene_shots(scene_id, ordering);

CREATE TABLE scene_compilations (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    scene_id TEXT NOT NULL,
    input_json TEXT NOT NULL CHECK (json_valid(input_json)),
    compilation_json TEXT NOT NULL CHECK (json_valid(compilation_json)),
    export_path TEXT NOT NULL,
    export_sha256 TEXT NOT NULL CHECK (length(export_sha256) = 64),
    created_at TEXT NOT NULL,
    FOREIGN KEY (project_id) REFERENCES projects(id),
    FOREIGN KEY (scene_id) REFERENCES world_scenes(id)
);
CREATE INDEX idx_scene_compilations_scene ON scene_compilations(scene_id, created_at DESC);

-- ---------------------------------------------------------------------------
-- Deterministic legacy migration (P8 `scenes` -> `world_scenes`)
-- ---------------------------------------------------------------------------
--
-- Mapping rule for each legacy scene row S (evaluated once, in legacy
-- created_at/id order):
--
--   1. If a derived row `'wsc-' || S.id` already exists, that is the target
--      (rerun safety).
--   2. Else, if exactly ONE pre-existing (non-derived) authoritative scene in
--      the same project has the same title, that scene is the target
--      (unambiguous attach; covers projects that authored the same scene on
--      both subsystems).
--   3. Else (no match, or AMBIGUOUS: two or more same-titled authoritative
--      scenes, or two legacy scenes sharing a title), a fresh derived row
--      `'wsc-' || S.id` is created for this legacy scene alone. Legacy rows
--      are never merged into an ambiguous target and never silently attached
--      to a guessed scene; nothing is discarded (the legacy tables remain).
--
-- Derived ordinals use `(max ordinal + 1) + S.rowid` so two rows inserted in
-- the same statement can never collide on UNIQUE(project_id, ordinal);
-- later scenes continue from MAX+1.

INSERT INTO world_scenes (id, project_id, ordinal, title, summary, world_id, world_asset_version_id, created_at, updated_at)
SELECT
    'wsc-' || s.id,
    s.project_id,
    (SELECT COALESCE(MAX(ws.ordinal), -1) + 1 FROM world_scenes ws WHERE ws.project_id = s.project_id)
      + s.rowid,
    s.title,
    '',
    NULL,
    s.world_asset_version_id,
    s.created_at,
    s.updated_at
FROM scenes s
WHERE NOT EXISTS (
    SELECT 1 FROM world_scenes ws WHERE ws.id = 'wsc-' || s.id
)
AND (SELECT COUNT(*) FROM world_scenes ws2
     WHERE ws2.project_id = s.project_id AND ws2.title = s.title
       AND ws2.id NOT LIKE 'wsc-%') <> 1;

-- Resolve world_id for the derived rows: the legacy world version must
-- currently be the canonical version of that World's plate asset.
UPDATE world_scenes
SET world_id = (
    SELECT w.id FROM worlds w
    JOIN assets a ON a.id = w.world_plate_asset_id
    WHERE a.project_id = world_scenes.project_id
      AND a.canonical_version_id = world_scenes.world_asset_version_id
)
WHERE id LIKE 'wsc-%' AND world_id IS NULL AND world_asset_version_id IS NOT NULL;

-- Target of one legacy scene: the unique non-derived same-title authoritative
-- scene when there is exactly one; otherwise the derived row.
-- `legacy_target(s.project_id, s.id)` expressed inline (twice below).
--
-- Move shots onto the mapped target. Shot ids are preserved.
INSERT INTO scene_shots (id, scene_id, ordering, duration_seconds, keyframe_asset_version_id, intent, action, camera, created_at, updated_at)
SELECT
    sh.id,
    CASE
        WHEN (SELECT COUNT(*) FROM world_scenes ws2
              WHERE ws2.project_id = s.project_id AND ws2.title = s.title
                AND ws2.id NOT LIKE 'wsc-%') = 1
        THEN (SELECT ws3.id FROM world_scenes ws3
              WHERE ws3.project_id = s.project_id AND ws3.title = s.title
                AND ws3.id NOT LIKE 'wsc-%')
        ELSE 'wsc-' || s.id
    END,
    sh.ordering, sh.duration_seconds, sh.keyframe_asset_version_id,
    sh.intent, sh.action, sh.camera, sh.created_at, sh.updated_at
FROM shots sh
JOIN scenes s ON s.id = sh.scene_id
WHERE NOT EXISTS (SELECT 1 FROM scene_shots ss WHERE ss.id = sh.id);

-- Move compilations likewise. Compilation ids are preserved.
INSERT INTO scene_compilations (id, project_id, scene_id, input_json, compilation_json, export_path, export_sha256, created_at)
SELECT
    cc.id,
    cc.project_id,
    CASE
        WHEN (SELECT COUNT(*) FROM world_scenes ws2
              WHERE ws2.project_id = s.project_id AND ws2.title = s.title
                AND ws2.id NOT LIKE 'wsc-%') = 1
        THEN (SELECT ws3.id FROM world_scenes ws3
              WHERE ws3.project_id = s.project_id AND ws3.title = s.title
                AND ws3.id NOT LIKE 'wsc-%')
        ELSE 'wsc-' || s.id
    END,
    cc.input_json, cc.compilation_json, cc.export_path, cc.export_sha256, cc.created_at
FROM cinema_compilations cc
JOIN scenes s ON s.id = cc.scene_id
WHERE NOT EXISTS (SELECT 1 FROM scene_compilations sc WHERE sc.id = cc.id);
