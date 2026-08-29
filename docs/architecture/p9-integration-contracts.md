# P9 integration contracts — frozen P0–P8 baseline

**Baseline inspected:** P8 cinema compiler, schema migrations through 18 (`0018_artifact_promotion_idempotency.sql`).
This document records existing contracts; it does not introduce a second service,
state store, compiler, or mutable authority. P9 code consumes these boundaries.

## Authority and transport

The Rust services and the project SQLite database are authoritative for all
business state. The Tauri command handler in `apps/desktop/src-tauri/src/lib.rs`
is the public desktop transport surface. React feature `api.ts` files are thin
`invokeCommand` adapters; local React state is display/interaction state only.
The frontend must not decide canonicality, derive a workflow transition, compute
QA outcomes, select retry eligibility, or persist a cinema compilation.

`packages/domain/src` is the shared TypeScript transport vocabulary. The backend
serializes public records in camelCase and its Rust enums in snake_case; a P9
change that crosses the webview boundary must update the owner and the matching
shared type/schema deliberately, rather than creating a competing shape.

## P0–P8 service map

| Phase | Owner and public boundary | Frozen contract |
| --- | --- | --- |
| P0 project kernel | `ProjectService::{create,open}`; `create_project`, `open_project`, `list_recent_projects` | A project root owns `project.db`; services reopen that root and read its project row to establish scope. Recent projects are a non-authoritative UI convenience cache. |
| P1 assets | `AssetService::{create_asset,list_assets,get_asset_with_versions,import_asset_version,promote_asset_version}`; corresponding asset commands | `assets.canonical_version_id`, not sort order or newest `version_number`, determines the canonical version. Import copies an immutable image into managed storage as `candidate`; promotion is explicit and atomically supersedes the previous canonical version. |
| P2 canon/TBD | `CanonService` entity/section/revision APIs; `canon::tbd::{create,list,resolve,reopen}`; canon commands | Sections are validated backend-side and revisioned. Locked sections supply workflow/cinema facts. TBD is `open` or `resolved`; reopening makes it `open` again. Protected open TBDs are inputs to the firewall, not content to invent. |
| P3 workflow | `WorkflowRuntime::{create_run,advance_run,approve_run_step,reject_run_step,cancel_run,get_run,list_runs}`; workflow commands | Runs/steps/events/approvals are durable backend state. A `WorkflowContextSnapshot` is version `1`, captures locked canon/current canonical asset refs, and is persisted in the workflow run DB record. The face-lock workflow additionally writes its snapshot artifact; QA/repair have their own persisted plan/context records and do not use that artifact path. Historical snapshots are evidence: never refresh or mutate them from current canon/assets. |
| P4 provider execution | `ProviderService`, `ProviderRegistry`, `GenerationProvider`, provider commands | The provider receives a compiled request, capabilities are checked before submission, and secrets stay in provider configuration/credential references. Attempts use `run_id:step_id:attempt_number` idempotency keys and durable `workflow_step_executions`/`provider_jobs`; retry is an explicit command allowed only after a failed attempt. |
| P5 durable generation | `GenerationService::{capture_provider_result,list_results,get_artifact_detail,promote_generated_artifact}`; generation commands | Provider output is materialized and SHA-256 verified before it is recorded as an `available` generated artifact. Every artifact has lineage. Promotion creates/imports an asset version; it never treats a remote URI or an unrecorded file as an asset. One result set per provider attempt and one promotion per artifact are enforced. |
| P6 visual QA/repair | `qa::workflow`, `QaService`, `RepairCompiler`, `repair_workflow`; QA commands | QA is scoped to the exact `asset_version_id`, with immutable plan/context snapshots and durable checks. A review on a succeeded run persists the check review status, derives effective results, and recomputes/persists the run `overall_status` without rewriting the model-reported check status. Repair requires succeeded QA, no unresolved uncertainty, and effective failures; it produces a provider-neutral edit request and a new child version—never an overwrite. |
| P7 scene context | `CinemaService::{create_scene,add_character_to_scene,add_prop_to_scene,create_shot,...}`; cinema commands | Scene records pin exact current-canonical asset version IDs at selection time. Cast looks/sheets, props, and optional world plate are validated to be in-project and the asset's current canonical pointer; they are not re-resolved to a later version. |
| P8 cinema compiler | `CinemaService::compile_scene`, `cinema::compiler::compile`, `cinema::export::export_compilation`; cinema commands | Compilation validates the scene, checks the TBD firewall, resolves locked behavioral/visual/world continuity, produces a deterministic provider-neutral prompt, atomically exports it under `prompts/cinema/`, then persists its input and output JSON plus hash. |

## Durable schema and statuses

Migrations are append-only entries in `db::migrations::MIGRATIONS`; migration
**18** is the current latest. Migration 17 consolidates the Scene domain: `scene_shots` and `scene_compilations` hang off the authoritative `world_scenes` aggregate; legacy P8 tables remain read-only. Migration 18 relaxes `artifact_promotions` uniqueness so content-deduped promotions (identical sha256 → one immutable version) record idempotently per artifact. Do not edit a shipped migration. The ordered P0–P8
schema is `0001_project_kernel` through `0012_cinema_compiler`.

| Domain | Values / storage invariant |
| --- | --- |
| Asset version status | `draft`, `generated`, `candidate`, `qa_failed`, `repairing`, `approved`, `canonical`, `superseded`. Asset types are `face_lock`, `outfit`, `character_sheet`, `world_plate`, `shot_keyframe`, `prop_plate`, `image`, `video`, `audio`; `video` and `audio` remain declared but rejected by Sprint-1 asset creation. |
| Workflow run | `created`, `running`, `waiting_for_approval`, `ready_for_execution`, `completed`, `rejected`, `cancelled`, `failed`. Step statuses: `pending`, `running`, `waiting`, `completed`, `skipped`, `failed`. |
| Provider lifecycle | `queued`, `submitted`, `running`, `succeeded`, `failed`, `cancellation_requested`, `cancelled`, `unknown`. An execution attempt identity is unique by `(workflow_run_id, step_definition_id, attempt_number)` and its idempotency key is globally unique. |
| Generated artifacts | Result sets and artifacts are image-only at P8. Capture status is `materializing`, `available`, or `failed`; `storage_path` is unique, artifact ordinal is unique within the result set, and the durable artifact is required before promotion. |
| QA | Run: `queued`, `running`, `succeeded`, `failed`, `cancelled`; overall: `pass`, `fail`, `needs_review`; check: `pass`, `fail`, `uncertain`, `not_applicable`; review: `unreviewed`, `confirmed`, `overridden_pass`, `overridden_fail`. |
| Cinema | A shot is `0 < duration_seconds <= 30`; ordering is unique per scene. A persisted compilation contains the submitted `CinemaCompileInput`, provider-neutral compiled JSON, atomic export path, and 64-char SHA-256. |

## Non-negotiable integration invariants

1. **Newest is not canonical.** UI may order versions newest-first for display,
   but it must mark/read `asset.canonicalVersionId`. Preconditions require both
   `asset_versions.status = canonical` and `assets.canonical_version_id = exact
   version id`.
2. **Exact scene references remain exact.** Scene rows store version IDs and
   `ensure_canonical_version` validates the selected version at write/compile
   time. P9 must not replace a stored scene reference with an asset's new
   canonical pointer.
3. **Workflow snapshots are immutable.** Use persisted workflow snapshot JSON
   for its workflow replay, review, and provenance; the face-lock workflow also
   has an on-disk snapshot artifact. QA/repair preserve their own plan/context
   JSON. Cinema compilation instead resolves current scene/canon state through
   `CinemaService`; do not substitute a workflow snapshot or recompute a saved
   workflow snapshot from live canon/assets.
4. **TBD firewall.** Cinema blocks a protected open project TBD or a protected
   open TBD scoped to a cast character. All open TBD topics are scrubbed from
   shot `intent`/`action` and scene `canon_notes` (including non-blocking ones),
   but the current compiler emits scene title and shot camera text unchanged.
   No task may fill a TBD with generated assumptions.
5. **Provider retry is explicit.** Cancellation is best effort provider-side;
   retry creates the next durable failed-attempt retry identity and is not an
   automatic replay.
6. **No phantom assets.** A provider URI/result is not an asset. Materialize,
   verify, persist result/artifact/lineage, then explicitly promote to a target
   asset. File-system orphan recovery quarantines unreferenced generated files.
7. **Repair creates a child version.** The source QA plan targets one exact
   source version; repairs preserve effective passes, change only effective
   failures, carry exact refs, and record `qa_repairs` provenance for a new
   `child_asset_version_id`.
8. **Cinema compilation is provider-neutral.** `ProviderNeutralCinemaPrompt`
   contains deterministic creative constraints, never provider/model IDs or
   credentials. Provider adaptation belongs downstream of this P8 contract.

## Existing architecture guards and P9 change rule

The baseline already has executable guards for these contracts: asset and
workflow prerequisite tests reject a newer non-canonical version; context tests
show snapshots survive later canon mutation; provider tests assert request
capability/secret boundaries; generation migration/acceptance tests enforce
durable lineage/promotion; QA repair tests enforce exact-source child repair;
and cinema tests cover canonical refs, TBD blocking and the current scoped
scrubbing behavior, duration totals, and provider-neutral compilation. A new
guard for the deliberate non-scrubbing of title/camera would only pass against
the existing P8 behavior and would not prevent a P9 duplicate authority; no
production behavior or redundant test was added merely to restate that scope.

The frontend audit found no duplicate mutable business authority: feature APIs
invoke backend commands, `AssetInspector` uses `canonicalVersionId` (while only
sorting for presentation), and QA Zod schemas are response-boundary validation.
The one intentional transport gap to preserve as a P9 integration concern is
that Rust `ProviderCapabilities` includes `supportsImageEdit`, while the shared
TypeScript `ProviderCapabilities` currently does not expose that field. Do not
invent a frontend decision from it; when P9 needs it, add the field atomically
to the shared type and its consumers with a contract test.

## P9 consumption checklist

- Call services through their Tauri commands; do not write project tables or
  managed media paths from React.
- Treat IDs in workflow snapshots, QA, scene records, and lineage as immutable
  refs; use the snapshot only in the workflow that captured it, and let cinema
  resolve its live scene/canon inputs through `CinemaService`.
- Add migrations only after version 18 and register them in `MIGRATIONS`.
- Keep provider-specific request shaping outside the provider-neutral workflow,
  generation, repair, and cinema contracts.
