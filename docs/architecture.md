# Architecture

**MVP IMPLEMENTED** — this document describes the architecture as shipped in
the current release. Anything that is planned but not present is marked
POST-MVP.

## Stack

| Layer | Technology |
|---|---|
| Desktop shell | Tauri 2 (Rust backend, system webview frontend) |
| Frontend | React 18 + TypeScript 5.8, Vite 6, Vitest 3 |
| Backend | Rust (edition 2021, MSRV 1.77.2) |
| Storage | SQLite via `rusqlite` (bundled), project-local `project.db` |
| IPC | Tauri `invoke` over `#[tauri::command]` |

## Monorepo layout

```
apps/desktop/           Tauri desktop app (frontend + Rust backend)
apps/desktop/src/       React/TypeScript frontend (features/, components/, lib/)
apps/desktop/src-tauri/ Rust backend (src/ modules, migrations/, tests/)
packages/domain/        @cinematic/domain shared TypeScript types
docs/                   Architecture, format, recovery, privacy, release docs
```

## Layering

Every feature follows the same three-layer split, so the IPC boundary is
thin and the rules live in one place.

1. **Domain types** (`packages/domain/src/*.ts`) — shared, validated shapes.
2. **Rust backend** (`apps/desktop/src-tauri/src/<feature>/`) —
   `commands.rs` (Tauri command boundary) → `service.rs` (business rules) →
   `repository.rs` (SQLite access). Migrations are append-only in
   `migrations/` and tracked in `schema_migrations`.
3. **React frontend** (`apps/desktop/src/features/<feature>/`) — thin
   `api.ts` wrappers over `invokeCommand`, presentation components, and
   co-located tests.

## Data authority (the invariants that hold the product together)

- **Canon is the structured authority.** Locked canon sections are the
  canonical facts; drafts are working state. Protected open TBDs are an
  explicit firewall: while one is open, dependent production actions block.
- **Canonical promotion is explicit and transactional.** `Newest` does not
  imply `canonical`; promoting a version supersedes the prior canonical in
  the same transaction.
- **Scenes pin exact asset versions.** Promoting a later World/Look version
  never silently rewrites a scene's pinned references; upgrading a scene is
  an explicit, user-driven restage.
- **Workflow snapshots are immutable.** Input, context, and output are
  captured and never rewritten after the run starts.
- **Provider selection is explicit, never silent.** Cloud/paid execution is
  disclosed (`ExecutionPrivacyBadge`), and retries of paid/cloud work are
  never silent.
- **Failed generation cannot create phantom assets.** A provider failure
  never materializes an output AssetVersion.
- **Credentials never enter project state.** Provider secrets are stored in
  the platform keychain only; projects, databases, snapshots, and diagnostics
  carry only credential references, and diagnostics are redacted.
- **P9 never introduces a second mutable production state.** The SQLite
  database remains the single source of truth; derived read-models
  (overview, health, provenance) are read-only.

## Health, provenance, recovery, diagnostics

- `integration/health.rs` — read-only 13-check integrity scan. Never repairs
  or rewrites.
- `integration/provenance.rs` — read-only graph traversal across
  asset versions, workflow runs, generations, QA runs, repairs, scenes,
  shots, and cinema compilations.
- `recovery/service.rs` — classifies incomplete jobs on open and explains
  what happened, what state is safe, and what the user can do.
- `diagnostics/` — redacted, media-free diagnostics bundle
  (`app-version.json`, `project-summary.json`, `database-version.json`,
  `project-health.json`, `active-jobs.json`, `recent-workflows.json`,
  `logs.txt`) plus a redacted structured log under `diagnostics/`.
