CREATE TABLE canon_entities (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  type TEXT NOT NULL CHECK (
    type IN (
      'story',
      'character',
      'location',
      'faction',
      'world_rule',
      'production_rules'
    )
  ),
  name TEXT NOT NULL,
  slug TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,

  FOREIGN KEY (project_id)
    REFERENCES projects(id),

  UNIQUE(project_id, type, slug)
);

CREATE INDEX idx_canon_entities_project_type
  ON canon_entities(project_id, type);

CREATE TABLE canon_sections (
  id TEXT PRIMARY KEY,
  canon_entity_id TEXT NOT NULL,
  section_key TEXT NOT NULL,
  value_json TEXT NOT NULL,
  status TEXT NOT NULL CHECK (
    status IN ('draft', 'locked')
  ),
  revision INTEGER NOT NULL CHECK (revision > 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  locked_at TEXT,

  FOREIGN KEY (canon_entity_id)
    REFERENCES canon_entities(id),

  UNIQUE(canon_entity_id, section_key)
);

CREATE INDEX idx_canon_sections_entity
  ON canon_sections(canon_entity_id);

CREATE INDEX idx_canon_sections_status
  ON canon_sections(status);

CREATE TABLE canon_section_revisions (
  id TEXT PRIMARY KEY,
  canon_section_id TEXT NOT NULL,
  revision INTEGER NOT NULL CHECK (revision > 0),
  value_json TEXT NOT NULL,
  status TEXT NOT NULL CHECK (
    status IN ('draft', 'locked')
  ),
  change_kind TEXT NOT NULL CHECK (
    change_kind IN ('create', 'edit', 'lock', 'unlock')
  ),
  reason TEXT,
  created_at TEXT NOT NULL,

  FOREIGN KEY (canon_section_id)
    REFERENCES canon_sections(id),

  UNIQUE(canon_section_id, revision)
);

CREATE INDEX idx_canon_revisions_section
  ON canon_section_revisions(
    canon_section_id,
    revision DESC
  );

CREATE TABLE canon_tbds (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  canon_entity_id TEXT,
  section_key TEXT,
  topic TEXT NOT NULL,
  note TEXT,
  protected INTEGER NOT NULL CHECK (
    protected IN (0, 1)
  ),
  status TEXT NOT NULL CHECK (
    status IN ('open', 'resolved')
  ),
  resolution_text TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  resolved_at TEXT,

  FOREIGN KEY (project_id)
    REFERENCES projects(id),

  FOREIGN KEY (canon_entity_id)
    REFERENCES canon_entities(id)
);

CREATE INDEX idx_canon_tbds_project_status
  ON canon_tbds(project_id, status);

CREATE INDEX idx_canon_tbds_protected_open
  ON canon_tbds(project_id, protected, status);
