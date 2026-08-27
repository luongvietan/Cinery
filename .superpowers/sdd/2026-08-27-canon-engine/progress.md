# SDD ledger — plan: 2026-08-27-canon-engine.md

## Setup and rulings

- Ruling: Continue from the dedicated `feat/canon-engine` checkout by snapshotting its current tracked and untracked changes into an isolated task worktree — the checkout is not a linked worktree, but it contains user-owned in-progress P1/P2 work that must not be discarded; cost if wrong: task commits will be based on a copied snapshot rather than the original dirty checkout.
- Ruling: Use migration version/file `0004_canon_engine.sql` because `0003_asset_version_dimensions.sql` already exists and is registered; preserve monotonic migration numbering and the plan’s schema exactly, adapted to the repository’s actual migration state; cost if wrong: a different migration number would collide with shipped asset schema or break restart migrations.

## Pre-flight scan

| Item | Relationship / files or interface | Finding | Ruling |
|---|---|---|---|
| Task 1 | Produces domain canon DTOs/schemas, Rust canon module, migration, exports consumed by Tasks 2–9 | Task 1’s specified `0003_canon_engine.sql` conflicts with the existing `0003_asset_version_dimensions.sql`; current dirty snapshot already contains partial Task 1 artifacts. | Use `0004_canon_engine.sql`; review and complete the snapshot as Task 1 before later tasks. |
| Task 2 | Consumes Task 1 `canon/mod.rs`, model/schema, migration; shares `canon/mod.rs`, `lib.rs` with Task 1 | Entity repository/service/commands depend on exact Task 1 types and app registration. | Task 1 is the gate; later agents must preserve the established Rust/domain contracts. |
| Task 3 | Shares Task 2 `repository.rs`, `service.rs`, `commands.rs`, frontend `api.ts`; consumes entity IDs and section schemas | Revision mutations depend on Task 2 project/entity lookup and Task 1 schema validation. | Keep all mutations service-owned and transactional; do not bypass repository/service boundaries. |
| Task 4 | Consumes Task 3 frontend API; shares project workspace and styles only indirectly | Story UI requires section lock/history APIs and must not invent a second state store. | Use Tauri wrappers from Task 3 and keep SQLite as source of truth. |
| Task 5 | Shares `CanonWorkspace.tsx` with Task 4 and Rust `service.rs` with Task 3 | Character navigation/editor must compose with Story workspace; visual-lock query depends on locked-section semantics. | Preserve existing Story routes and expose only locked visual locks to query helpers. |
| Task 6 | Shares `CanonWorkspace.tsx` with Tasks 4–5; consumes generic entity/section APIs from Tasks 2–3 | Additional categories must use the same lock/history state machine. | Implement category UI as thin clients over generic canon APIs; no category-specific persistence. |
| Task 7 | Shares `repository.rs`, `service.rs`, `commands.rs`, frontend `api.ts`, `CanonWorkspace.tsx` with Tasks 2–6 | TBD scope validation depends on entity/section lookup and UI must coexist with all canon tabs. | Enforce project ownership and existing-section checks server-side; preserve protected/open/resolved ordering. |
| Task 8 | Shares `service.rs`, `commands.rs`, frontend `api.ts` with Tasks 3/7; consumes all entity categories and TBD query | Export must read structured database state and represent missing/draft/locked values deterministically. | Export only from SQLite; keep stable ordering and atomic write semantics. |
| Task 9 | Consumes every prior service/query/export interface; touches acceptance docs/README only | Acceptance depends on all prior contracts and restart persistence. | Run only after Tasks 1–8 have reviewed commits; verify no P3/provider code leaks in. |
| Task 1 | Internal consistency: files, tests, schemas, migration, Rust module | Self-consistent after migration-number adaptation; tests must validate exact schema and migration registration. | Review against adapted repository paths. |
| Task 2 | Internal consistency: repository/service/commands/api and singleton tests | Self-consistent; singleton bootstrap must be idempotent and project-scoped. | Preserve stable ULIDs/slugs and ownership checks. |
| Task 3 | Internal consistency: revision module, service/repository/commands/API/tests | Self-consistent; lock/unlock/edit transitions all create contiguous section revisions. | No restore/deletion APIs in P2. |
| Task 4 | Internal consistency: Story components, project integration, CSS, tests | Self-consistent; read-only locked editors and history dialog must use real API states. | Keep UI deterministic and AI-free. |
| Task 5 | Internal consistency: Character components/service query/tests | Self-consistent; structured visual locks are character `visual_locks` section. | Enforce unique visual-lock keys via shared schema. |
| Task 6 | Internal consistency: category components/tests/workspace | Self-consistent; location/faction/world/production use generic semantics. | Production Rules remains a singleton. |
| Task 7 | Internal consistency: TBD persistence/service/UI/tests | Self-consistent; resolving never mutates referenced canon and reopening clears resolution fields. | Keep protected flag on reopen. |
| Task 8 | Internal consistency: exporter/command/API/button/tests | Self-consistent; exact ordering/status markers/atomic file write are public contract. | No timestamps in export body. |
| Task 9 | Internal consistency: acceptance test, verification guide, README | Self-consistent; acceptance asserts restart, locked mutation errors, history, queries, deterministic export. | Run full suite and debug build before completion. |

## Task progress

