CREATE TABLE worlds (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    canon_location_entity_id TEXT NOT NULL,
    world_plate_asset_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(project_id) REFERENCES projects(id),
    FOREIGN KEY(world_plate_asset_id) REFERENCES assets(id),
    UNIQUE(project_id, canon_location_entity_id)
);

CREATE TABLE world_scenes (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    title TEXT NOT NULL,
    summary TEXT NOT NULL DEFAULT '',
    world_id TEXT NULL,
    world_asset_version_id TEXT NULL,
    keyframe_asset_id TEXT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(project_id) REFERENCES projects(id),
    FOREIGN KEY(world_id) REFERENCES worlds(id),
    FOREIGN KEY(world_asset_version_id) REFERENCES asset_versions(id),
    FOREIGN KEY(keyframe_asset_id) REFERENCES assets(id),
    UNIQUE(project_id, ordinal)
);

CREATE TABLE world_scene_characters (
    id TEXT PRIMARY KEY,
    scene_id TEXT NOT NULL,
    character_entity_id TEXT NOT NULL,
    look_asset_version_id TEXT NOT NULL,
    sheet_asset_version_id TEXT NULL,
    notes TEXT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(scene_id) REFERENCES world_scenes(id) ON DELETE CASCADE,
    FOREIGN KEY(look_asset_version_id) REFERENCES asset_versions(id),
    FOREIGN KEY(sheet_asset_version_id) REFERENCES asset_versions(id),
    UNIQUE(scene_id, character_entity_id)
);

CREATE TABLE world_scene_props (
    id TEXT PRIMARY KEY,
    scene_id TEXT NOT NULL,
    prop_asset_version_id TEXT NOT NULL,
    label TEXT NULL,
    notes TEXT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(scene_id) REFERENCES world_scenes(id) ON DELETE CASCADE,
    FOREIGN KEY(prop_asset_version_id) REFERENCES asset_versions(id),
    UNIQUE(scene_id, prop_asset_version_id)
);

CREATE TABLE scene_tbd_bindings (
    id TEXT PRIMARY KEY,
    scene_id TEXT NOT NULL,
    canon_tbd_id TEXT NOT NULL,
    topic_snapshot TEXT NOT NULL,
    note_snapshot TEXT NULL,
    decision TEXT NOT NULL CHECK(decision IN ('preserve_unknown','not_applicable')),
    justification TEXT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(scene_id) REFERENCES world_scenes(id) ON DELETE CASCADE,
    UNIQUE(scene_id, canon_tbd_id)
);

CREATE TABLE scene_reference_events (
    id TEXT PRIMARY KEY,
    scene_id TEXT NOT NULL,
    reference_kind TEXT NOT NULL,
    assignment_id TEXT NULL,
    action TEXT NOT NULL,
    from_version_id TEXT NULL,
    to_version_id TEXT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(scene_id) REFERENCES world_scenes(id)
);
