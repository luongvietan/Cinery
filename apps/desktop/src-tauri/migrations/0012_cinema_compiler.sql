CREATE TABLE scenes (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  title TEXT NOT NULL CHECK (length(trim(title)) BETWEEN 1 AND 160),
  world_asset_version_id TEXT,
  canon_notes TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (project_id) REFERENCES projects(id),
  FOREIGN KEY (world_asset_version_id) REFERENCES asset_versions(id)
);
CREATE INDEX idx_scenes_project ON scenes(project_id, created_at DESC);

CREATE TABLE scene_characters (
  scene_id TEXT NOT NULL,
  character_entity_id TEXT NOT NULL,
  look_asset_version_id TEXT NOT NULL,
  sheet_asset_version_id TEXT,
  display_order INTEGER NOT NULL CHECK (display_order >= 0),
  FOREIGN KEY (scene_id) REFERENCES scenes(id) ON DELETE CASCADE,
  FOREIGN KEY (character_entity_id) REFERENCES canon_entities(id),
  FOREIGN KEY (look_asset_version_id) REFERENCES asset_versions(id),
  FOREIGN KEY (sheet_asset_version_id) REFERENCES asset_versions(id),
  PRIMARY KEY (scene_id, character_entity_id)
);

CREATE TABLE scene_props (
  scene_id TEXT NOT NULL,
  prop_asset_version_id TEXT NOT NULL,
  display_order INTEGER NOT NULL CHECK (display_order >= 0),
  FOREIGN KEY (scene_id) REFERENCES scenes(id) ON DELETE CASCADE,
  FOREIGN KEY (prop_asset_version_id) REFERENCES asset_versions(id),
  PRIMARY KEY (scene_id, prop_asset_version_id)
);

CREATE TABLE shots (
  id TEXT PRIMARY KEY,
  scene_id TEXT NOT NULL,
  ordering INTEGER NOT NULL CHECK (ordering >= 0),
  duration_seconds REAL NOT NULL CHECK (duration_seconds > 0 AND duration_seconds <= 30),
  keyframe_asset_version_id TEXT,
  intent TEXT NOT NULL CHECK (length(trim(intent)) BETWEEN 1 AND 240),
  action TEXT,
  camera TEXT,
  generated_video_asset_version_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (scene_id) REFERENCES scenes(id) ON DELETE CASCADE,
  FOREIGN KEY (keyframe_asset_version_id) REFERENCES asset_versions(id),
  UNIQUE(scene_id, ordering)
);
CREATE INDEX idx_shots_scene ON shots(scene_id, ordering);

CREATE TABLE cinema_compilations (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  scene_id TEXT NOT NULL,
  input_json TEXT NOT NULL CHECK (json_valid(input_json)),
  compilation_json TEXT NOT NULL CHECK (json_valid(compilation_json)),
  export_path TEXT NOT NULL,
  export_sha256 TEXT NOT NULL CHECK (length(export_sha256)=64),
  created_at TEXT NOT NULL,
  FOREIGN KEY (project_id) REFERENCES projects(id),
  FOREIGN KEY (scene_id) REFERENCES scenes(id)
);
CREATE INDEX idx_cinema_compilations_scene ON cinema_compilations(scene_id, created_at DESC);
