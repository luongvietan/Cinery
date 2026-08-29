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
-- 1. Every legacy scene with no title match in `world_scenes` becomes a new
--    authoritative scene (next free ordinal, empty summary). Its world
--    binding is preserved only when the pinned world plate version is still
--    the canonical version of a World's plate asset; the owning World then
--    provides the world_id.
-- 2. Legacy shots keep their ids and are re-parented onto the mapped scene.
-- 3. Legacy compilations keep their ids and are re-parented likewise.
-- Legacy tables keep their original rows (read-only archive); matching by
-- title is deterministic and order-stable (earliest ordinal wins).

INSERT INTO world_scenes (id, project_id, ordinal, title, summary, world_id, world_asset_version_id, created_at, updated_at)
SELECT
    'wsc-' || s.id,
    s.project_id,
    (SELECT COALESCE(MAX(ws.ordinal) + 1, 0) FROM world_scenes ws WHERE ws.project_id = s.project_id),
    s.title,
    '',
    NULL,
    s.world_asset_version_id,
    s.created_at,
    s.updated_at
FROM scenes s
WHERE NOT EXISTS (
    SELECT 1 FROM world_scenes ws
    WHERE ws.project_id = s.project_id AND ws.title = s.title
)
AND NOT EXISTS (
    -- id collision guard: mapping ids are derived, so this cannot repeat
    SELECT 1 FROM world_scenes ws WHERE ws.id = 'wsc-' || s.id
);

-- Resolve world_id for the freshly created rows: the legacy world version
-- must currently be the canonical version of that World's plate asset.
UPDATE world_scenes
SET world_id = (
    SELECT w.id FROM worlds w
    JOIN assets a ON a.id = w.world_plate_asset_id
    WHERE a.project_id = world_scenes.project_id
      AND a.canonical_version_id = world_scenes.world_asset_version_id
)
WHERE id LIKE 'wsc-%' AND world_id IS NULL AND world_asset_version_id IS NOT NULL;

-- Move shots onto the authoritative scene that carries the legacy scene's
-- title (existing match or the one created above).
INSERT INTO scene_shots (id, scene_id, ordering, duration_seconds, keyframe_asset_version_id, intent, action, camera, created_at, updated_at)
SELECT sh.id, ws.id, sh.ordering, sh.duration_seconds, sh.keyframe_asset_version_id, sh.intent, sh.action, sh.camera, sh.created_at, sh.updated_at
FROM shots sh
JOIN scenes s ON s.id = sh.scene_id
JOIN world_scenes ws ON ws.project_id = s.project_id AND ws.title = s.title
WHERE NOT EXISTS (SELECT 1 FROM scene_shots ss WHERE ss.id = sh.id);

-- Move compilations likewise.
INSERT INTO scene_compilations (id, project_id, scene_id, input_json, compilation_json, export_path, export_sha256, created_at)
SELECT cc.id, cc.project_id, ws.id, cc.input_json, cc.compilation_json, cc.export_path, cc.export_sha256, cc.created_at
FROM cinema_compilations cc
JOIN scenes s ON s.id = cc.scene_id
JOIN world_scenes ws ON ws.project_id = s.project_id AND ws.title = s.title
WHERE NOT EXISTS (SELECT 1 FROM scene_compilations sc WHERE sc.id = cc.id);
