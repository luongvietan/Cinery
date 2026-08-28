# Task 2 report — project production overview

## Changed files

- `apps/desktop/src-tauri/src/integration/readiness.rs` — deterministic,
  read-only P0–P8 readiness projection plus health/activity/job summaries.
- `apps/desktop/src-tauri/src/integration/commands.rs` and `mod.rs` — the
  Tauri transport for `get_project_overview`.
- `apps/desktop/src-tauri/src/lib.rs` — integration module and command
  registration.
- `apps/desktop/src-tauri/tests/project_overview.rs` — focused behavioral
  coverage for every Task 2 readiness scenario.
- `packages/domain/src/integration.ts` and `index.ts` — shared camelCase
  transport vocabulary.
- `apps/desktop/src/features/overview/*` — thin adapter, actionable overview,
  and UI coverage.
- `apps/desktop/src/features/projects/ProjectWorkspace.tsx` and
  `apps/desktop/src/styles/app.css` — Overview-first desktop workspace entry
  point and visual treatment.

The pre-existing QA Rust changes, `ProjectWorkspace.test.tsx` change, and graft
cache changes were not modified or staged.

## Decisions

- Rust/SQLite remain the sole readiness authority. React receives the complete
  derived projection through one Tauri command and only routes the user to the
  existing Canon, Assets, or Production workspace.
- Canonical readiness requires the authoritative asset canonical pointer and a
  `canonical` version status; newest candidates are ignored.
- A staged scene is evaluated from its persisted exact references. The overview
  checks that those referenced records still exist, but never re-resolves them
  to the asset's current canonical pointer, so superseded historical scene refs
  do not appear missing.
- No migration or new state store was introduced. Recent activity, active jobs,
  health, and readiness are all read-only queries over existing durable tables.

## TDD red/green evidence

- RED (Rust): `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
  --test project_overview` failed because the new `integration` module did not
  exist (`E0433`).
- GREEN (Rust): the same focused test passed with all 11 required readiness
  scenarios.
- RED (UI): `pnpm --filter @cinematic/desktop test -- ProjectOverview.test.tsx`
  failed because `ProjectOverview` did not exist.
- GREEN (UI): the same focused test passed with 2 UI behavior tests.

## Tests

- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test
  project_overview` — passed (11).
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test
  cinema_compiler` — passed (2).
- `pnpm --filter @cinematic/domain test` — passed.
- `pnpm --filter @cinematic/desktop test` — passed (40).
- `pnpm --filter @cinematic/desktop build` — passed.
- Full `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` was
  attempted but cannot compile the pre-existing dirty QA integration test
  `tests/qa_repository.rs` (136 errors, including missing `Into` imports and
  QA symbols). Task 2 focused Rust tests compile and pass independently.

## Commit

`feat: add project production overview`

## Concerns

- The full Rust suite remains blocked by the separate, unmodified dirty QA
  worktree changes noted above.
- A current-canonical scene reference is still required by P8 when compiling a
  *new* cinema prompt. The overview intentionally preserves historical scene
  reference readiness rather than preemptively rewriting or relabeling it.
