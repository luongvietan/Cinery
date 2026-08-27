CREATE TABLE assets (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  type TEXT NOT NULL CHECK (
    type IN (
      'face_lock',
      'outfit',
      'character_sheet',
      'world_plate',
      'shot_keyframe',
      'prop_plate',
      'image',
      'video',
      'audio'
    )
  ),
  label TEXT NOT NULL,
  owner_entity_id TEXT,
  canonical_version_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (project_id) REFERENCES projects(id)
);

CREATE TABLE asset_versions (
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
    mime_type IN ('image/png', 'image/jpeg', 'image/webp')
  ),
  byte_size INTEGER NOT NULL CHECK (byte_size >= 0),
  parent_version_id TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY (asset_id) REFERENCES assets(id),
  FOREIGN KEY (parent_version_id) REFERENCES asset_versions(id),
  UNIQUE(asset_id, version_number),
  UNIQUE(asset_id, sha256)
);

CREATE INDEX idx_asset_versions_asset_id ON asset_versions(asset_id);
CREATE INDEX idx_assets_project_id ON assets(project_id);
