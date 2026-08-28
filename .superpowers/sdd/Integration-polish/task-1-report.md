# Task 1 report — freeze P0–P8 integration contracts

## Changed files

- `docs/architecture/p9-integration-contracts.md` — P0–P8 public boundary,
  migration/status, authority, and invariant map.
- `.superpowers/sdd/Integration-polish/task-1-report.md` — this implementation
  report.

No Rust, TypeScript, migrations, or existing tests were changed. The pre-existing
QA Rust changes and graft cache changes were left untouched.

## Contract decisions

- SQLite/Rust services are the single mutable authority; Tauri is the desktop
  transport boundary and React API files are adapters.
- Canonicality is the exact `assets.canonical_version_id` pointer, never newest
  version ordering.
- Workflow snapshots, scene version references, QA plans/contexts, lineage, and
  cinema compilation records are historical immutable evidence.
- The current schema terminates at migration 12 (`0012_cinema_compiler.sql`);
  future migrations append only.
- The contract records all declared asset/workflow/provider/QA statuses; P8
  cinema compilation is provider-neutral and protected TBDs remain a firewall.
- No new type or architecture test was added: existing acceptance/unit tests
  already directly guard the required invariants, and adding a duplicate guard
  would create maintenance-only authority.
- The frontend audit found adapter/response-validation logic only. A documented
  future concern is that backend provider capabilities include
  `supportsImageEdit` but the shared TypeScript capability type does not.

## Red/green evidence

There was no production or test code change, so no TDD red phase applied. The
documentation records existing, executable guards rather than duplicating them.
Fresh repository verification is recorded below after the documentation edit.

## Tests

- `pnpm test` — passed: 26 domain tests and 38 desktop tests (64 total).
- `cargo test -j 1` from `apps/desktop/src-tauri` — passed: 125 library
  tests and 74 integration tests (199 total); doc-tests also passed.

## Commit

`chore: freeze p0-p8 integration contracts` (the report is intentionally kept
under an ignored planning directory and was explicitly included in the scoped
commit).

## Concerns

- The TypeScript `ProviderCapabilities` omits Rust's `supportsImageEdit`; P9
  must bridge this only if it consumes image-edit capability, with one shared
  transport contract and test.

## Round 1 contract corrections

- Corrected snapshot scope: `WorkflowContextSnapshot` is durable in the
  workflow DB record; only the face-lock workflow writes its snapshot artifact.
  QA/repair persist their own plan/context records, and cinema resolves its live
  scene/canon inputs through `CinemaService` rather than from a workflow
  snapshot.
- Corrected QA review semantics: it persists a review status, derives effective
  check results, and recomputes/persists `qa_runs.overall_status` while retaining
  the model-reported check status.
- Corrected TBD scrubbing scope: the cinema compiler scrubs open topics from
  shot intent/action and scene canon notes only. It currently emits scene title
  and shot camera text unchanged.
- Architecture-guard assessment: an additional test asserting title/camera
  non-scrubbing would pass immediately against unchanged P8 behavior and would
  only freeze a known compiler scope, not guard against duplicate authority. No
  new production/test code was added; the existing cinema compiler test remains
  the focused guard for deterministic action-text scrubbing.

## Round 1 verification

- `cargo test -j 1 --test cinema_compiler --test cinema_export` — passed: 5
  focused cinema tests.
- `cargo test -j 1` from `apps/desktop/src-tauri` — passed again: 125 library
  tests and 74 integration tests (199 total); doc-tests also passed.
