# Project Format

**MVP RELEASE CANDIDATE.** Every project is a plain directory on disk. There is no
server and no cloud dependency for local work; only the cloud execution
steps themselves require network access.

## Directory layout

A project root always contains:

```
project.yaml            bootstrap manifest (format, project_id, schema_version)
project.db              SQLite database (the single source of truth)
assets/                 original imported media
thumbnails/             generated WebP thumbnails
canon/  characters/  worlds/  props/  scenes/
prompts/  generations/  exports/  diagnostics/
```

`project.yaml` is a bootstrap marker only: it exists so a directory can be
identified as a project without opening its database. The mutable project
metadata (name, timestamps) lives only in SQLite.

## SQLite schema

- Append-only migrations in `apps/desktop/src-tauri/migrations/0001..0013.sql`.
- Applied migrations are recorded in `schema_migrations`.
- Foreign keys are enforced. `asset_versions` never stores media bytes;
  it stores relative paths into `assets/` and `thumbnails/`.
- Composite performance indexes were added in migration `0013` based on the
  actual query paths (workflow steps/events by run, QA by workflow, artifact
  lineage by artifact, workflow approvals by run+step, workflow runs by
  project+status, asset versions by asset+status, QA runs by project+version).

## Media

- Only PNG, JPEG, and WebP images are supported in the MVP.
- Media files are kept outside the database (never in DB blobs).
- Thumbnails are generated at import time and used in grids; full-resolution
  images are loaded only in the single-item inspector and candidate previews.
- Grids lazy-load and async-decode thumbnails.

## Determinism

- IDs are ULIDs (sortable, unique).
- The Story Bible Markdown is a deterministic export; it is never the
  machine source of truth.
- Compiling a scene produces a durable provider-neutral prompt persisted to
  `exports/` with an SHA-256, so a compilation can be re-inspected later.

## Privacy boundary

- Credentials are never stored in the project; only a credential reference
  (environment-variable name) is recorded.
- Diagnostics bundles are redacted and exclude media by default.
