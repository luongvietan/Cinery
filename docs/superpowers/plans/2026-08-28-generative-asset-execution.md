# Generative Asset Execution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close Cinery's first end-to-end production loop: an approved Character Builder request executes through the existing provider abstraction, stores durable generated candidates with immutable lineage, and explicitly promotes a selected candidate through AssetService.

**Architecture:** Extend the existing Workflow Runtime and Provider Service; do not create a second generation state machine. Add a generation bounded context for result sets, durable artifact capture, lineage, recovery, and promotion orchestration. Keep provider credentials backend-only and keep AssetService as the only creator of AssetVersions.

**Tech Stack:** Rust/Tauri, rusqlite migrations, serde, sha2/image, TypeScript/React, Vitest, existing P1-P4 services and command envelope.

**Spec:** `.superpowers/sdd/2026-08-28-generative-asset-execution/spec.md`

## Global Constraints

- Provider output is not an Asset Version; promotion is an explicit user action.
- Asset versions and generated artifacts are immutable; retries create new attempts/artifacts.
- Generation inputs pin exact source AssetVersion IDs, canon snapshot, skill/workflow versions, compiled request, and provider attempt/model.
- Remote provider URLs must be materialized into project-managed storage before artifacts become available.
- Promotion is idempotent and must reuse AssetService; direct `INSERT INTO asset_versions` from generation is forbidden.
- Workflow Runtime remains the only execution authority; DryRun compiles and approves without remote generation or fake AssetVersions.
- Secrets must not enter generation records, lineage, audit payloads, IPC responses, or frontend state/errors.

### Task 1: Freeze P5 contracts and integration map

**Files:**
- Create: `.superpowers/sdd/2026-08-28-generative-asset-execution/task-1-report.md`
- Inspect: `apps/desktop/src-tauri/src/workflow/{model.rs,execution.rs,runtime.rs,repository.rs}`
- Inspect: `apps/desktop/src-tauri/src/providers/{model.rs,service.rs,repository.rs}`
- Inspect: `apps/desktop/src-tauri/src/assets/{service.rs,repository.rs}`
- Inspect: `apps/desktop/src-tauri/migrations/0005_workflow_runtime.sql`, `0006_provider_integrations.sql`, `0007_provider_audit_events.sql`

- [ ] Record exact existing run, step, compiled request, provider-attempt, AssetService, storage, and audit identifiers.
- [ ] Run focused existing workflow/provider/asset tests and record the baseline.

### Task 2: Domain models and persistence

**Files:**
- Create: `packages/domain/src/generation.ts`, `packages/domain/src/lineage.ts` and their tests.
- Modify: `packages/domain/src/index.ts`.
- Create: `apps/desktop/src-tauri/migrations/0008_generated_artifacts.sql`, `0009_artifact_lineage.sql`.
- Create: `apps/desktop/src-tauri/src/generation/{mod.rs,model.rs,repository.rs,error.rs}`.
- Modify: `apps/desktop/src-tauri/src/db/migrations.rs` and `src/lib.rs`.
- Test: Rust migration and repository tests.

- [ ] Write failing serialization/schema tests for result sets, artifacts, source links, lineage, promotion uniqueness, and project ownership.
- [ ] Run domain and Rust tests to confirm the new contracts fail before implementation.
- [ ] Implement the minimum typed models, two non-destructive migrations, repository queries, and command registration scaffolding.
- [ ] Verify migration upgrade from a P4 database and all uniqueness/foreign-key invariants.

### Task 3: Durable artifact storage

**Files:**
- Create: `apps/desktop/src-tauri/src/generation/storage.rs`.
- Test: storage unit tests in the same module and `src-tauri/tests/generation_acceptance.rs`.

- [ ] Write failing tests for deterministic project-relative paths, temp-write/flush/hash/atomic-finalize ordering, missing files, hash mismatch, and injected write failure.
- [ ] Implement project-managed `generated/<run>/<attempt>/<ordinal>` storage with atomic temp rename and SHA-256 verification.
- [ ] Verify failed capture leaves no available artifact and does not alter Assets.

### Task 4: Lineage engine

**Files:**
- Create: `apps/desktop/src-tauri/src/generation/lineage.rs`.
- Test: lineage unit tests and redaction tests.

- [ ] Write failing tests requiring workflow run/step, provider attempt/id/model, compiled request identity/hash, skill version, and exact source AssetVersion for Character Builder.
- [ ] Implement deterministic lineage construction from existing runtime/provider records, rejecting incomplete/conflicting identity and omitting credentials.
- [ ] Verify canonical pointer changes do not mutate stored source version IDs.

### Task 5: Workflow → Provider → Artifact bridge

**Files:**
- Modify: `apps/desktop/src-tauri/src/providers/{model.rs,adapter.rs,service.rs}` only where needed for normalized durable image bytes.
- Modify: `apps/desktop/src-tauri/src/workflow/{runtime.rs,executor.rs,commands.rs,repository.rs}`.
- Create: `apps/desktop/src-tauri/src/generation/service.rs` and `commands.rs`.
- Test: provider/workflow/generation acceptance tests.

- [ ] Write failing acceptance tests for approved face-lock execution, deterministic mock four-output capture, capability rejection before submission, provider failure, and DryRun no-generation behavior.
- [ ] Implement `GenerationService` as the bridge invoked by the approved existing workflow step; persist one result set per provider attempt, capture each normalized output, and persist lineage before exposing candidates.
- [ ] Preserve P4 provider audit/lifecycle/cancellation/retry semantics and ensure provider code knows nothing about AssetVersion promotion.
- [ ] Verify artifacts are durable, hash-valid, and selectable only after capture and lineage succeed.

### Task 6: Artifact → Asset Version promotion

**Files:**
- Modify: `apps/desktop/src-tauri/src/assets/service.rs` only to expose a safe file-based import boundary if required.
- Modify: `apps/desktop/src-tauri/src/generation/{service.rs,repository.rs,commands.rs}`.
- Test: promotion unit and acceptance tests.

- [ ] Write failing tests for `setCanonical=false`, `setCanonical=true`, failed promotion cleanup, project mismatch, unavailable/corrupt artifact, and duplicate IPC retry.
- [ ] Implement `promote_generated_artifact` by copying through AssetService, recording one `artifact_promotions` row, and applying canonical selection through existing canon logic.
- [ ] Verify one artifact yields at most one AssetVersion, the source canonical version remains unchanged unless explicitly requested, and retry returns the existing promotion.

### Task 7: Recovery, errors, and audit

**Files:**
- Modify: `apps/desktop/src-tauri/src/generation/{error.rs,recovery.rs,service.rs,repository.rs}`.
- Modify: `apps/desktop/src-tauri/src/error.rs` and provider audit integration.
- Test: capture failure, orphan, recovery, and secret-regression tests.

- [ ] Write failing tests for missing/corrupt artifacts, orphan temp/final files, capture failure after provider success, promotion retry, and sentinel secret absence.
- [ ] Implement machine-readable generation errors, deterministic orphan quarantine/cleanup, recoverable artifact state, and generation audit events using redacted payloads.
- [ ] Verify provider failures remain provider failures and capture failures remain inspectable generation failures.

### Task 8: Production UI

**Files:**
- Create: `apps/desktop/src/features/production/{ProductionWorkspace.tsx,CharacterBuilderOperation.tsx,GenerationPreparation.tsx,GenerationProgress.tsx,GenerationResults.tsx,GenerationResultCard.tsx,PromoteArtifactDialog.tsx}`.
- Create: `apps/desktop/src/features/production/api.ts` and tests.
- Modify: `apps/desktop/src/features/projects/ProjectWorkspace.tsx`.

- [ ] Write failing component tests for Production navigation, source/version selection, review/approve, progress, keyboard result selection, and explicit promotion.
- [ ] Implement the golden Character Builder flow over generation Tauri commands, preserving existing Workflow Runtime approval and cancellation.
- [ ] Verify no credentials or raw provider URLs are rendered, progress is accessible, focus returns after dialogs, and retry never replaces existing results.

### Task 9: Asset lineage UI

**Files:**
- Modify: `apps/desktop/src-tauri/src/assets/{model.rs,repository.rs,commands.rs}` to return optional generated metadata.
- Modify: `apps/desktop/src/features/assets/{AssetInspector.tsx,api.ts}` and styles.
- Create: `apps/desktop/src/features/production/ArtifactLineagePanel.tsx` and tests.

- [ ] Write failing tests for GENERATED badges, provider/model metadata, lineage details, and historical source immutability.
- [ ] Implement minimal version metadata and a read-only lineage panel without redesigning Assets or Canon.
- [ ] Verify imported versions remain unchanged and generated versions trace to the original source AssetVersion.

### Task 10: Acceptance hardening and verification

**Files:**
- Create: `apps/desktop/src-tauri/tests/generation_acceptance.rs`, `.superpowers/sdd/2026-08-28-generative-asset-execution/task-*.md` reports as evidence accumulates, and `verification.md`.

- [ ] Run domain tests, frontend tests, Rust unit/acceptance tests, production build, Tauri build, and `git diff --check`.
- [ ] Exercise golden path, canonical change mid-run, provider failure, capture failure, cancellation, IPC retry, and secret-redaction acceptance scenarios.
- [ ] Review every P5 invariant against code and tests, document any bounded deviations, and only then report completion.
