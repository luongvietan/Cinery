# Architecture

**MVP RELEASE CANDIDATE** — this document describes the architecture as shipped in
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
services/ai-worker/     Python sidecar placeholder (JSON-RPC 2.0 over stdio)
docs/                   Architecture, format, recovery, privacy, release docs
```

### Master plan §6 package mapping

The master plan (§6) *recommends* eleven separate TypeScript packages
(`domain`, `database`, `project-kernel`, `canon`, `assets`, `workflows`,
`skills`, `prompt-compiler`, `providers`, `qa`, `shared`). The shipped
architecture concentrates the same responsibilities as Rust modules inside
`apps/desktop/src-tauri/src/` plus one shared TypeScript domain package,
because the production engine is implemented in Rust, not TypeScript.
The §6 package boundaries map onto the shipped modules as follows:

| §6 package (recommended) | Shipped module(s) |
|---|---|
| `domain` (pure types/entities/state) | `packages/domain/src/*.ts` (IPC contract types) + `apps/desktop/src-tauri/src/*/model.rs` |
| `database` (schema, migrations, repositories) | `apps/desktop/src-tauri/src/db/` + `migrations/` + each feature's `repository.rs` |
| `project-kernel` | `apps/desktop/src-tauri/src/project/` |
| `canon` | `apps/desktop/src-tauri/src/canon/` |
| `assets` | `apps/desktop/src-tauri/src/assets/` |
| `workflows` (state-machine orchestration) | `apps/desktop/src-tauri/src/workflow/` |
| `skills` (definition format, registry, runtime) | `apps/desktop/src-tauri/src/skills/` + `workflow/runtime.rs` |
| `prompt-compiler` | `apps/desktop/src-tauri/src/workflow/compiler.rs` + `cinema/compiler.rs` |
| `providers` (adapters, routing) | `apps/desktop/src-tauri/src/providers/` |
| `qa` (checks, comparison, repair) | `apps/desktop/src-tauri/src/qa/` |
| `shared` | `packages/domain` (single shared package covers this) |

This deviation follows the master plan's own engineering rules (§57.12
"prefer boring technology"): the boundaries exist as Rust module
boundaries with the same layering invariants (`commands.rs` → `service.rs` →
`repository.rs`), without the overhead of publishing eleven packages whose
only consumer is one desktop app.

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

## Production router (master plan §13)

`router/` routes a free-text production intent to a versioned skill
operation:

- **Deterministic scorer** matches intent text against each operation's
  `intentExamples` (stop-word filtered, ≥50% word overlap threshold) and
  runs `evaluate_feasibility`, which mirrors the workflow prerequisite
  checks against current project state.
- **LLM classifier (optional)** — when the project has a configured Text
  (LLM) custom provider with a stored credential, a chat completion
  *proposes* an operation id. Code always re-validates the proposal
  against the registry and prerequisites, and the suggestion can never
  outrank a deterministic match (§13, §53: the LLM may suggest, code must
  validate). Any LLM failure or missing service degrades to the
  deterministic matcher.
- Exposed as `route_production_intent`; the AI Director bar on the
  Overview renders the top suggestion and its blockers.

## Providers (master plan §14)

`providers/` registers a `GenerationProvider` per adapter behind one
interface (`submit` / `poll` / `cancel` / `fetch_result`):

- `dry_run` — writes the compiled request as an artifact.
- `mock` — deterministic in-process image generator (tests/diagnostics,
  hidden from user-facing run forms).
- `fake_async_video` — deterministic async video provider (submit →
  polling with progress → MP4 data-URI result) used by the offline video
  golden path.
- `openai` — image generation/editing built from the `openai-compatible`
  preset (credential from keychain; `OPENAI_API_KEY` is a developer-time
  fallback only).
- Any other id resolves to a **declarative custom provider** built from
  the stored definition and vault credential: operation-based endpoints
  (`image.generate`, `image.edit`, `video.generate`,
  `video.imageToVideo`, `validate`), sync or async jobs, bearer/header/
  query auth, per-operation polling, and secret redaction throughout.

## Python AI worker (master plan §5.5)

`services/ai-worker/` is a placeholder Python 3.12 sidecar speaking
JSON-RPC 2.0 over stdio (`ping`, `health`). Post-MVP responsibilities
(local VLM, embeddings, ComfyUI plumbing) extend it as new methods; the
protocol boundary stays stable and business rules remain in Rust.

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
