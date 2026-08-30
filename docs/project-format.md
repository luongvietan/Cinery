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

- Append-only migrations in `apps/desktop/src-tauri/migrations/0001..0021.sql`.
- Applied migrations are recorded in `schema_migrations`.
- Foreign keys are enforced. `asset_versions` never stores media bytes;
  it stores relative paths into `assets/` and `thumbnails/`.
- Composite performance indexes were added in migration `0013` based on the
  actual query paths (workflow steps/events by run, QA by workflow, artifact
  lineage by artifact, workflow approvals by run+step, workflow runs by
  project+status, asset versions by asset+status, QA runs by project+version).
- Migration `0020` widened the media CHECKs to `image | video` (MP4) — the
  video pipeline's persistence contract. Migrations `0020`/`0021` rebuild
  tables (SQLite cannot ALTER a CHECK): the runner disables `foreign_keys`
  outside the transaction and verifies `foreign_key_check` inside it.
- Migration `0021` added `scene_shots.generated_video_asset_version_id`,
  a shot's exact immutable video version pin (mirrors the keyframe pin).

## Media

- Images: PNG, JPEG, WebP. Video: MP4 (`video/mp4`, ISO-BMFF `ftyp`
  container check). `audio` is not supported.
- Media files are kept outside the database (never in DB blobs).
- Thumbnails are generated at import time for images and used in grids;
  full-resolution images are loaded only in the single-item inspector and
  candidate previews. Video versions carry no thumbnail (the UI renders
  `<video preload="metadata">` previews instead).
- Grids lazy-load and async-decode thumbnails.

## Determinism

- IDs are ULIDs (sortable, unique).
- The Story Bible Markdown is a deterministic export; it is never the
  machine source of truth.
- Compiling a scene produces a durable provider-neutral prompt persisted to
  `exports/` with an SHA-256, so a compilation can be re-inspected later.

## Privacy boundary

- Credentials are never stored in the project; only a `keyring://` vault
  reference is recorded (the OS credential manager holds the secret).
- Diagnostics bundles are redacted and exclude media by default.
