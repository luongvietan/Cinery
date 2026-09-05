PRAGMA foreign_keys = ON;

-- Sequence-first flow state (Joey contract): one explicitly-approved
-- workflow record per authoritative Scene. The Scene aggregate remains the
-- sole creative authority; this table only persists the human-authored
-- director brief and the workflow's stage/approval state, keyed by scene.
CREATE TABLE sequence_flows (
    scene_id TEXT PRIMARY KEY REFERENCES world_scenes(id) ON DELETE CASCADE,
    brief_json TEXT NOT NULL CHECK (json_valid(brief_json)),
    stage TEXT NOT NULL CHECK (stage IN (
        'draft', 'brief_locked', 'references_ready', 'prompt_approved',
        'generating', 'in_review', 'canonical_selected', 'ready_for_edit')),
    approved_compilation_id TEXT,
    canonical_shot_id TEXT,
    extension_direction TEXT CHECK (extension_direction IN ('prequel', 'sequel')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
