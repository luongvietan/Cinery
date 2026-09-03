# P10.3 Shot Video QA Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add durable, immutable-evidence Video QA for generated Shot video candidates without changing QA's non-authoritative relationship to promotion and generation.

**Architecture:** Add `video-qa@1.0.0` as a separate built-in skill that reuses the existing P6 QA persistence, review model, and workflow runtime. Resolve video evidence exclusively from P10.2 immutable provenance, compile a deterministic video-specific plan, evaluate it through strict adapters, then expose candidate-local results in `ShotImageToVideo`.

**Tech Stack:** Rust/Tauri, SQLite, TypeScript/React, Vitest, existing WorkflowRuntime and QA services.

**Spec:** `.superpowers/specs/P10.3 — Shot Video QA Implementation Specification.md`

## Global Constraints

- Register exactly `video-qa@1.0.0` / `asset.run_video_qa`; do not change `visual-qa@1.0.0` semantics.
- Target exactly one persisted video `asset_version_id`; never infer historical evidence from the current Shot pin, latest asset, or Canon state.
- QA is evidence only: it must not mutate Canon, mutate a Shot, promote, regenerate, or spend external credits without explicit approval.
- Persist complete immutable context and deterministic check plan before adapter execution; no DB migration unless a demonstrated invariant requires one.
- Reject unsupported provenance with `VIDEO_QA_PROVENANCE_UNSUPPORTED`; never fall back to mutable state or arbitrary imported videos.
- Use stable check IDs, strict one-to-one output normalization, raw-result-preserving human review, atomic success persistence, and no silent re-execution on restart.
- Cloud approval must disclose exact target, provider/model, CLOUD, relevant references, and whether direct video or sampled frames leave the device. Credentials must never persist in QA state.
- Promotion remains explicit for PASS/FAIL/NEEDS_REVIEW and generation remains manual after FAIL. Do not add a local polling timer.
- All implementation follows TDD: capture focused RED and GREEN evidence in the per-task report.

---

### Task 0: Prove packaged temporal-evidence capability

**Files:**
- Modify: `apps/desktop/src-tauri/Cargo.toml` only if an already-vendored dependency is required.
- Create: `apps/desktop/src-tauri/tests/video_qa_evidence_path.rs`
- Create: `docs/release-evidence/2026-09-01-p10-3-shot-video-qa.md`

**Produces:** A tested, package-compatible evidence extraction boundary with no PATH-only `ffmpeg` dependency, and an explicit evidence-mode contract for later adapter work.

- [ ] Write a failing test that creates a fixture video and asserts the chosen production-compatible extractor returns deterministic opening/temporal evidence or a typed evidence-unsupported error.
- [ ] Run the focused test and record the RED result.
- [ ] Implement the narrow extractor/bundled-resource binding; do not introduce a system-installed binary requirement.
- [ ] Run the focused test, record GREEN, and document the actual production evidence path plus any limitation truthfully.
- [ ] Commit with `feat: prove video QA evidence path`.

### Task 1: Video QA domain and versioned skill contract

**Files:**
- Modify: `apps/desktop/src-tauri/src/qa/models.rs`, `apps/desktop/src-tauri/src/qa/mod.rs`, `apps/desktop/src-tauri/src/skills/registry.rs`, `apps/desktop/src-tauri/src/workflow/runtime.rs`
- Create: `apps/desktop/src-tauri/src/skills/builtin/video_qa.rs`
- Test: existing QA/registry/runtime Rust tests or focused new tests beside the affected module.

**Consumes:** Task 0 evidence-mode decision.
**Produces:** `QaMediaKind::{Image,Video}`, additive video check vocabulary, `RunVideoQaInput`, and a resolvable `video-qa@1.0.0` workflow definition.

- [ ] Write RED tests for registry resolution, `run_video_qa` input validation, and serialization of legacy QA records as `image`.
- [ ] Implement additive serde-compatible domain types and the video skill workflow (`validate-input`, `video_qa_context`, `video_qa_v1`, approval, execute, complete).
- [ ] Add runtime dispatch only for the new resolver/compiler/operation; preserve existing visual QA routing unchanged.
- [ ] Run focused Rust tests, confirm visual QA regression tests remain green, and commit `feat: register video QA workflow`.

### Task 2: Immutable generated-video provenance context

**Files:**
- Create: `apps/desktop/src-tauri/src/qa/video_context.rs`
- Modify: `apps/desktop/src-tauri/src/qa/mod.rs`, `apps/desktop/src-tauri/src/qa/models.rs`, relevant generation/provenance repository helpers.
- Test: `apps/desktop/src-tauri/tests/video_qa_context.rs`

**Produces:** `ResolvedVideoQaContext` containing exact target identity/hash, generation origin, source K1, relevant immutable references, and frozen generation intent.

- [ ] Write RED tests for non-video, missing file, unsupported provenance, K1-not-current-K2 resolution, Shot mutation stability, and Canon mutation stability.
- [ ] Resolve candidate provenance through existing generated artifact/result/provider attempt/workflow records, never through `Shot.generated_video_asset_version_id` or current keyframe pin.
- [ ] Persist only IDs/content hashes as historical identity; derive paths only for execution.
- [ ] Run the focused context tests and commit `feat: resolve immutable video QA context`.

### Task 3: Deterministic video check planning

**Files:**
- Create: `apps/desktop/src-tauri/src/qa/video_check_planner.rs`
- Modify: `apps/desktop/src-tauri/src/qa/models.rs`, `apps/desktop/src-tauri/src/qa/mod.rs`
- Test: `apps/desktop/src-tauri/tests/video_qa_planner.rs`

**Produces:** Stable video check IDs and deterministic blocking policy for integrity, continuity, conditional identity/reference/motion/camera checks, coherence, cuts, flicker, deformation, watermark, and artifacts.

- [ ] Write RED tests that the same frozen context gives byte-stable plans; camera and identity checks appear only with their exact evidence; missing planned checks cannot pass.
- [ ] Implement planner using only Task 2 context and the specified stable ID shapes, keeping planner-owned `blocking` immutable to the evaluator.
- [ ] Test raw aggregation: blocking fail → fail; otherwise applicable uncertain → needs_review; otherwise all applicable pass → pass.
- [ ] Commit `feat: plan deterministic video QA checks`.

### Task 4: Evidence adapters and strict normalization

**Files:**
- Create: `apps/desktop/src-tauri/src/qa/adapters/video_mock.rs`, `apps/desktop/src-tauri/src/qa/adapters/video_production.rs`
- Modify: `apps/desktop/src-tauri/src/qa/adapters/mod.rs`, `apps/desktop/src-tauri/src/qa/normalizer.rs`, `apps/desktop/src-tauri/src/qa/models.rs`
- Test: `apps/desktop/src-tauri/tests/video_qa_normalization.rs`

**Produces:** Mock and one packaged production adapter that receive exact evidence, emit typed results, and reject missing/extra/duplicate/invalid evaluator output atomically.

- [ ] Write RED cases for unknown, missing, duplicate, invalid-status, out-of-range-confidence, wrong-schema, denied-extra-field, and malformed response failures.
- [ ] Generalize normalizer only where image behavior is regression-tested; reconcile exactly one evaluator output with every planned check.
- [ ] Bind production media transfer to Task 0's evidence mechanism and expose direct-video/sampled-frame mode for approval disclosure.
- [ ] Run focused tests plus image-normalizer regressions and commit `feat: add strict video QA adapters`.

### Task 5: Workflow execution, persistence, review, and recovery

**Files:**
- Create: `apps/desktop/src-tauri/src/qa/video_workflow.rs`
- Modify: `apps/desktop/src-tauri/src/qa/repository.rs`, `apps/desktop/src-tauri/src/qa/workflow.rs`, `apps/desktop/src-tauri/src/workflow/runtime.rs`, `apps/desktop/src-tauri/src/qa/mod.rs`
- Test: `apps/desktop/src-tauri/tests/video_qa_workflow.rs`

**Consumes:** Tasks 1–4.
**Produces:** Durable context/plan-before-execution, approval, active-run deduplication, atomic completion, generic QA history/restoration, failures without phantom checks, and raw-preserving overrides.

- [ ] Write RED tests for approval rejection (zero invocations), duplicate active request, adapter failure, invalid response, atomic completion, terminal explicit rerun, restart restoration, and override preserving raw result/effective aggregate.
- [ ] Reuse P6 QA tables and review APIs after verifying exact-AssetVersion storage needs no migration; record a migration ruling in release evidence.
- [ ] Persist execution location and safe diagnostics, handle interrupted non-durable execution conservatively, and do not add a separate runtime or job system.
- [ ] Run focused workflow and existing P6/P10.2 regressions, then commit `feat: persist video QA workflow results`.

### Task 6: Shot-local Video QA UI

**Files:**
- Create: `apps/desktop/src/features/qa/VideoQaPanel.tsx`, `apps/desktop/src/features/qa/VideoQaPanel.test.tsx`
- Modify: `apps/desktop/src/features/qa/api.ts`, `apps/desktop/src/features/qa/types.ts`, `apps/desktop/src/features/qa/schemas.ts`, `apps/desktop/src/features/scenes/ShotImageToVideo.tsx`, `apps/desktop/src/test/StatefulTauriFacade.ts`

**Produces:** Candidate-version-local QA status/history, run/rerun/review actions, raw-vs-human-vs-effective presentation, and restoration via existing workflow lifecycle.

- [ ] Write RED frontend tests for completed candidate visibility, status variants, no duplicate action during active run, restoration, override evidence, terminal rerun, rapid click singleton, V1 history after V2, explicit promotion under QA fail, and no auto-generation/promotion.
- [ ] Add `createVideoQaWorkflow` using generic `create_workflow_run`; do not add a dedicated creation command.
- [ ] Mount the panel under the exact candidate in `ShotImageToVideo`, reuse existing status observation, and do not use `setInterval`.
- [ ] Run focused frontend tests and commit `feat: add shot-local video QA panel`.

### Task 7: End-to-end immutable-history acceptance

**Files:**
- Create: `apps/desktop/src-tauri/tests/shot_video_qa_golden_path.rs`
- Modify: focused fixtures/support only when needed.

**Produces:** Golden-path and mutation-acceptance coverage proving V1→K1 evidence, review, promotion separation, and historical stability after K2/V2/Canon changes.

- [ ] Write the golden path test from project/Scene/Shot/K1/I2V/V1 through QA approval/mock execution/review/explicit promotion.
- [ ] Assert target V1, source K1, candidate-local QA ownership, no automatic promotion, then mutate keyframe/Canon/generate/promote V2 and re-read V1 QA.
- [ ] Run the focused integration test with background/restart guards and commit `test: cover video QA immutable golden path`.

### Task 8: Regression gates and release evidence

**Files:**
- Modify: `docs/release-evidence/2026-09-01-p10-3-shot-video-qa.md`

- [ ] Run `pnpm -r test`, Rust tests with `CARGO_BUILD_JOBS=1`, TypeScript no-emit, Vite build, clippy `-D warnings`, format check, `git diff --check`, and Tauri production build.
- [ ] Perform and truthfully record the manual GUI walkthrough if the environment permits; otherwise mark it not performed. Mark clean-install OPEN unless actually run.
- [ ] Record branch, baseline, commits, evidence architecture, migration ruling, focused RED/GREEN evidence, all gates, manual result, bundle result, and open risks.
- [ ] Commit `docs: record P10.3 video QA release evidence`.
