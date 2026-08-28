# Provider Integrations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a provider execution layer that runs approved P3 image requests through a common adapter contract, persists attempts/jobs/artifacts, supports recovery/cancellation/retry, and exposes minimal provider configuration and runtime status UI.

**Architecture:** Keep `ExecutionRequest` and P3 workflow definitions provider-neutral. Add a synchronous Rust adapter contract because the current Tauri runtime is synchronous; asynchronous provider jobs are represented by durable submit/poll/fetch transitions and reconciled by explicit runtime calls. `ProviderRegistry` resolves configured provider IDs, while `ArtifactIngestion` converts successful provider bytes into existing candidate asset versions.

**Tech Stack:** Rust 1.77+, Tauri 2, `serde`, `serde_json`, `rusqlite`, `chrono`, `ulid`, `sha2`, `image`, `ureq` for the reference HTTP adapter; React + TypeScript + Vitest + React Testing Library for provider UI.

**Spec:** `.superpowers/sdd/2026-08-28-provider-integrations/spec.md`

## Global Constraints

- The provider integration layer MUST remain subordinate to P3 runtime semantics.
- Provider-neutral compiled intent MUST remain deterministic and MUST NOT gain provider/model fields.
- Credentials MUST NOT enter Canon, skill definitions, snapshots, compiled requests, audit events, or exported workflow files.
- The system MUST NOT silently fall back from one provider/model to another.
- Unsupported capabilities MUST fail before provider submission with a normalized error.
- Remote provider URLs are temporary inputs; canonical output is persisted locally.
- DryRun MUST use the same provider abstraction as production adapters.
- Automatic submission retry is allowed only when idempotency is guaranteed or non-acceptance is known.
- Real network tests are opt-in and MUST NOT run in the default local or CI suite.
- Existing P3 tests and behavior must remain passing.

---

## File Structure

- Create `apps/desktop/src-tauri/src/providers/model.rs` for provider-neutral IDs, capabilities, requests, jobs, results, and lifecycle types.
- Create `apps/desktop/src-tauri/src/providers/error.rs` for stable provider error taxonomy and redacted diagnostics.
- Create `apps/desktop/src-tauri/src/providers/adapter.rs` for the `GenerationProvider` contract and execution translation.
- Create `apps/desktop/src-tauri/src/providers/registry.rs` for provider resolution and configuration-aware adapter construction.
- Create `apps/desktop/src-tauri/src/providers/dry_run.rs` for the common-contract DryRun adapter.
- Create `apps/desktop/src-tauri/src/providers/mock.rs` for deterministic contract and recovery fixtures.
- Create `apps/desktop/src-tauri/src/providers/openai.rs` and `apps/desktop/src-tauri/src/providers/http.rs` for the first real image adapter and injectable HTTP transport.
- Create `apps/desktop/src-tauri/src/providers/repository.rs` for configuration, execution attempt, and provider job persistence.
- Create `apps/desktop/src-tauri/src/providers/service.rs` for provider configuration, validation, and lifecycle orchestration.
- Create `apps/desktop/src-tauri/src/workflow/ingestion.rs` for downloaded-output validation and candidate asset registration.
- Create `apps/desktop/src-tauri/migrations/0006_provider_integrations.sql` for additive P4 tables.
- Modify `apps/desktop/src-tauri/src/workflow/runtime.rs` only at the execute/recovery boundaries; leave P3 compile, approval, and Canon snapshot code unchanged.
- Modify `apps/desktop/src-tauri/src/workflow/recovery.rs` to reconcile durable provider jobs instead of failing every interrupted execution.
- Modify `apps/desktop/src-tauri/src/workflow/repository.rs` and `model.rs` only to expose execution metadata in the existing run detail.
- Modify `apps/desktop/src-tauri/src/error.rs`, `workflow/mod.rs`, `lib.rs`, and `Cargo.toml` for public wiring and stable IPC errors.
- Create `apps/desktop/src-tauri/src/providers/tests.rs` and `apps/desktop/src-tauri/tests/provider_acceptance.rs` for contract and crash-window coverage.
- Modify `packages/domain/src/workflow.ts` and `apps/desktop/src/features/workflows/api.ts` for provider DTOs and commands.
- Create `apps/desktop/src/features/providers/ProviderSettings.tsx` and `apps/desktop/src/features/providers/ProviderSettings.test.tsx` for masked credential/configuration UX.
- Modify `apps/desktop/src/features/workflows/WorkflowRunView.tsx`, `WorkflowWorkspace.tsx`, `format.ts`, and `styles/app.css` for normalized provider status, model review, retry, cancel, and accessible errors.

### Task 1: Define the provider contract and normalized types

**Files:**
- Create: `apps/desktop/src-tauri/src/providers/model.rs`
- Create: `apps/desktop/src-tauri/src/providers/error.rs`
- Create: `apps/desktop/src-tauri/src/providers/adapter.rs`
- Create: `apps/desktop/src-tauri/src/providers/mod.rs`
- Modify: `apps/desktop/src-tauri/src/workflow/mod.rs`
- Modify: `apps/desktop/src-tauri/src/error.rs`
- Test: inline unit tests in `providers/model.rs`, `providers/error.rs`, and `providers/adapter.rs`

**Interfaces:**
- `pub trait GenerationProvider`: `capabilities`, `submit`, `poll`, `cancel`, `fetch_result`.
- `ProviderExecutionRequest::from_execution_request(run_id, step_id, compiled_request_id, provider_id, model_id, idempotency_key, request)`.
- `ProviderCapabilities::supports(&self, request: &ProviderExecutionRequest) -> Result<(), ProviderError>`.
- `ProviderLifecycle::{Queued, Submitted, Running, Succeeded, Failed, CancellationRequested, Cancelled, Unknown}`.
- `ProviderErrorKind` covers authentication, authorization, invalid request, unsupported capability, rate limit, quota, unavailable, network, timeout, remote failure/not found, malformed response, download/validation, cancelled, and unknown errors.

- [ ] **Step 1: Write failing serialization and capability tests**

Assert stable snake_case lifecycle/error values, provider-neutral request fields, no secret fields, and rejection of unsupported video/reference capabilities before submission.

- [ ] **Step 2: Run the focused Rust tests and verify they fail for missing types**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml providers -- --nocapture`

Expected: compile failures naming the missing provider types.

- [ ] **Step 3: Implement the minimal provider contract and error taxonomy**

Use synchronous trait methods matching the existing runtime; keep provider-specific payloads out of the shared request type and redact diagnostic text before storing it.

- [ ] **Step 4: Run focused tests and the existing domain tests**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml providers -- --nocapture` and `pnpm --filter @cinematic/domain test`.

Expected: provider tests and all existing domain tests pass.

- [ ] **Step 5: Commit the contract**

Run: `git add apps/desktop/src-tauri/src/providers apps/desktop/src-tauri/src/workflow/mod.rs apps/desktop/src-tauri/src/error.rs && git commit -m "feat: define provider execution contract"`

### Task 2: Add durable provider configuration, attempts, and jobs

**Files:**
- Create: `apps/desktop/src-tauri/migrations/0006_provider_integrations.sql`
- Create: `apps/desktop/src-tauri/src/providers/repository.rs`
- Create: `apps/desktop/src-tauri/src/providers/service.rs`
- Modify: `apps/desktop/src-tauri/src/db/migrations.rs`
- Modify: `apps/desktop/src-tauri/src/error.rs`
- Test: migration and repository unit tests

**Interfaces:**
- `ProviderConfigRecord { provider_id, enabled, credential_reference, default_model, endpoint, request_timeout_seconds, polling_interval_seconds }`.
- `ExecutionAttemptRecord { id, run_id, step_id, attempt_number, compiled_request_id, provider_id, model_id, idempotency_key, status, provider_job_id, normalized_error, started_at, completed_at }`.
- `ProviderJobRecord { id, execution_id, provider_id, provider_job_id, status, submitted_at, updated_at }`.
- Repository functions `upsert_provider_config`, `get_provider_config`, `list_provider_configs`, `create_attempt`, `persist_job`, `update_attempt_status`, and `find_active_attempt`.

- [ ] **Step 1: Write failing migration/reopen tests**

Create an in-memory migrated database, assert all P4 tables and constraints exist, insert a config/attempt/job, reopen a file-backed project, and assert records survive.

- [ ] **Step 2: Run migration tests and verify the new schema is absent**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml migrations::tests::provider -- --nocapture`

Expected: failures because migration version 6 and the P4 tables do not exist.

- [ ] **Step 3: Add migration 0006 and repository mappings**

Use additive tables with explicit status checks, foreign keys to `workflow_runs`, immutable attempt rows, unique `(run_id, step_id, attempt_number)`, and unique idempotency keys per run.

- [ ] **Step 4: Run migration, repository, and reopen tests**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml migrations providers::repository -- --nocapture`.

Expected: all selected tests pass and migration rerun remains idempotent.

- [ ] **Step 5: Commit durable storage**

Run: `git add apps/desktop/src-tauri/migrations/0006_provider_integrations.sql apps/desktop/src-tauri/src/db/migrations.rs apps/desktop/src-tauri/src/providers/repository.rs apps/desktop/src-tauri/src/providers/service.rs apps/desktop/src-tauri/src/error.rs && git commit -m "feat: persist provider executions and configuration"`

### Task 3: Make DryRun and deterministic mock providers use the common contract

**Files:**
- Create: `apps/desktop/src-tauri/src/providers/dry_run.rs`
- Create: `apps/desktop/src-tauri/src/providers/mock.rs`
- Modify: `apps/desktop/src-tauri/src/workflow/executor.rs`
- Modify: `apps/desktop/src-tauri/src/workflow/artifacts.rs`
- Modify: `apps/desktop/src-tauri/src/providers/registry.rs`
- Test: provider contract tests and existing workflow tests

**Interfaces:**
- `DryRunProvider` returns a deterministic synthetic artifact without network access.
- `MockImageProvider` accepts a scripted lifecycle and returns deterministic PNG bytes or a scripted normalized error.
- `ProviderRegistry::builtin()` registers `dry_run` and `mock` without provider-specific branches in `WorkflowRuntime`.

- [ ] **Step 1: Write contract tests against the mock fixture**

Cover capability declaration, submit/poll/fetch success, cancellation, unknown status mapping, malformed response, and deterministic output bytes.

- [ ] **Step 2: Run the tests and verify they fail**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml providers::tests -- --nocapture`

Expected: missing adapter implementations or contract methods.

- [ ] **Step 3: Implement DryRun and Mock adapters**

Route the existing DryRun artifact writer through `GenerationProvider`; preserve the exact P3 JSON/text artifacts and do not add network access to DryRun.

- [ ] **Step 4: Run all existing workflow tests and provider contract tests**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml workflow providers -- --nocapture`.

Expected: existing P3 tests pass and the common provider contract is green.

- [ ] **Step 5: Commit common backends**

Run: `git add apps/desktop/src-tauri/src/providers apps/desktop/src-tauri/src/workflow/executor.rs apps/desktop/src-tauri/src/workflow/artifacts.rs && git commit -m "feat: run dryrun through provider contract"`

### Task 4: Implement the reference OpenAI image adapter

**Files:**
- Create: `apps/desktop/src-tauri/src/providers/http.rs`
- Create: `apps/desktop/src-tauri/src/providers/openai.rs`
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/src/providers/registry.rs`
- Test: `apps/desktop/src-tauri/src/providers/openai.rs` and fake-server contract tests

**Interfaces:**
- `OpenAiImageProvider::from_config(config, secret_resolver)` reads `OPENAI_API_KEY` or an explicitly configured backend credential reference; no frontend payload contains secret bytes.
- It translates only supported provider-neutral image request data into `/v1/images/generations`, rejects unsupported references/parameters before HTTP, and normalizes response schema/errors.
- `HttpTransport` is injectable so tests use a deterministic local fake server; real smoke tests are gated by `PROVIDER_SMOKE_TEST=1`.

- [ ] **Step 1: Write fake-server tests for translation and lifecycle normalization**

Assert authorization is added only inside the adapter, request body has prompt/model/size/quality fields derived from stable input, provider 401/429/5xx and malformed JSON map to stable errors, and returned image URLs are preserved only as transient result references.

- [ ] **Step 2: Run focused adapter tests and verify they fail**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml providers::openai -- --nocapture`

Expected: missing HTTP transport and OpenAI adapter behavior.

- [ ] **Step 3: Implement the adapter and transport**

Use `ureq` with bounded timeouts, response-size limits, redacted error extraction, and explicit model selection. Treat the provider's immediate image response as a submitted job whose first poll is terminal success, keeping the common lifecycle intact.

- [ ] **Step 4: Run adapter tests and the no-network default suite**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml providers -- --nocapture`.

Expected: fake-server tests pass; no test requires `OPENAI_API_KEY`.

- [ ] **Step 5: Commit the reference adapter**

Run: `git add apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/Cargo.lock apps/desktop/src-tauri/src/providers && git commit -m "feat: add reference image provider adapter"`

### Task 5: Integrate provider lifecycle with P3 runtime and artifact ingestion

**Files:**
- Create: `apps/desktop/src-tauri/src/workflow/ingestion.rs`
- Modify: `apps/desktop/src-tauri/src/workflow/runtime.rs`
- Modify: `apps/desktop/src-tauri/src/workflow/repository.rs`
- Modify: `apps/desktop/src-tauri/src/workflow/model.rs`
- Modify: `apps/desktop/src-tauri/src/assets/service.rs`
- Test: `apps/desktop/src-tauri/tests/provider_acceptance.rs`

**Interfaces:**
- `WorkflowRuntime::execute_ready` resolves provider/model from persisted configuration, creates an attempt, persists the provider job before treating submission as complete, polls to terminal state, and emits meaningful audit events.
- `ArtifactIngestion::persist_provider_result(project_root, run_id, step_id, attempt, result, expected_output)` downloads/accepts bounded bytes, validates MIME and non-zero content, creates or finds the target candidate asset, and returns durable artifact metadata.
- Existing approval checks remain the only gate before `execute_ready`.

- [ ] **Step 1: Write failing end-to-end acceptance tests**

Use a temporary project and mock provider to assert unapproved execution is rejected, approved execution creates a new attempt/job/candidate asset, Canon snapshot JSON is unchanged, and repeated recovery finds the existing artifact instead of duplicating it.

- [ ] **Step 2: Run acceptance tests and verify they fail before provider integration**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_acceptance -- --nocapture`

Expected: no provider attempt/job/artifact is created by the current P3 DryRun-only runtime.

- [ ] **Step 3: Implement common runtime execution and ingestion**

Keep compilation and approval code intact; replace only the execute-step backend lookup and use `AssetService::import_asset_version` for validated local output. Link output metadata to the immutable attempt and emit provider events through the existing event repository.

- [ ] **Step 4: Run P3 and P4 acceptance tests**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml workflow -- --nocapture` and `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_acceptance -- --nocapture`.

Expected: all existing P3 tests and new runtime/artifact tests pass.

- [ ] **Step 5: Commit runtime integration**

Run: `git add apps/desktop/src-tauri/src/workflow apps/desktop/src-tauri/src/assets/service.rs apps/desktop/src-tauri/tests/provider_acceptance.rs && git commit -m "feat: integrate provider results with workflow artifacts"`

### Task 6: Add recovery, retry, cancellation, and redaction guarantees

**Files:**
- Modify: `apps/desktop/src-tauri/src/workflow/recovery.rs`
- Modify: `apps/desktop/src-tauri/src/providers/service.rs`
- Modify: `apps/desktop/src-tauri/src/providers/repository.rs`
- Modify: `apps/desktop/src-tauri/src/error.rs`
- Test: `apps/desktop/src-tauri/tests/provider_acceptance.rs` and provider unit tests

**Interfaces:**
- `ProviderService::recover_project(project_root)` reconciles active jobs by persisted provider/job IDs and never resubmits an accepted job by default.
- `ProviderService::retry_execution(project_root, run_id, step_id)` creates a new immutable attempt only for retryable failures.
- `ProviderService::cancel_execution(project_root, run_id, step_id)` requests provider cancellation when supported, otherwise marks local cancellation and prevents late completion from advancing the workflow.
- `redact_secret(value)` removes API keys, authorization headers, and signed query values from errors/log/audit payloads.

- [ ] **Step 1: Write failing crash-window, cancellation, retry, and leakage tests**

Cover crashes before submit, after durable submit, during polling, after provider completion, during artifact persistence, provider-supported/unsupported cancellation, late completion, retryable/non-retryable errors, immutable previous attempts, and secret absence from serialized records.

- [ ] **Step 2: Run the focused tests and verify they fail**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml provider_acceptance -- --nocapture`.

Expected: interrupted jobs are currently marked failed and no retry/cancel APIs exist.

- [ ] **Step 3: Implement reconciliation and policy**

Persist every state needed for recovery, classify retryability by `ProviderErrorKind`, create deterministic idempotency keys from run/step/attempt, and keep late results detached after local cancellation.

- [ ] **Step 4: Run all recovery/security tests**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml provider_acceptance providers workflow -- --nocapture`.

Expected: all crash windows, cancellation, retry, and redaction tests pass.

- [ ] **Step 5: Commit reliability behavior**

Run: `git add apps/desktop/src-tauri/src/workflow/recovery.rs apps/desktop/src-tauri/src/providers apps/desktop/src-tauri/src/error.rs apps/desktop/src-tauri/tests/provider_acceptance.rs && git commit -m "feat: recover retry and cancel provider executions"`

### Task 7: Expose narrow Tauri commands and provider DTOs

**Files:**
- Modify: `apps/desktop/src-tauri/src/providers/service.rs`
- Create: `apps/desktop/src-tauri/src/providers/commands.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `packages/domain/src/workflow.ts`
- Modify: `apps/desktop/src/features/workflows/api.ts`
- Test: Rust command/service tests and TypeScript API tests

**Interfaces:**
- Commands: `list_providers`, `get_provider_capabilities`, `get_provider_configuration_status`, `configure_provider`, `remove_provider_credentials`, `validate_provider_configuration`, `list_provider_models`, `cancel_workflow_execution`, `retry_workflow_execution`.
- Commands accept project paths and stable IDs only; credential status returns `configured`, `providerId`, and `credentialReference`, never plaintext secrets.

- [ ] **Step 1: Write failing command DTO and IPC tests**

Assert serialized provider/model/capability/status shapes and assert configuration responses never contain secret values.

- [ ] **Step 2: Run focused frontend/Rust tests and verify missing command wiring**

Run: `pnpm --filter @cinematic/desktop test -- src/features/workflows/api.test.ts` and `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml providers::commands -- --nocapture`.

Expected: missing provider command functions and DTOs.

- [ ] **Step 3: Implement narrow commands and domain types**

Use existing `invokeCommand` conventions and stable camelCase DTOs; route all validation through `ProviderService`.

- [ ] **Step 4: Run focused tests**

Run: `pnpm --filter @cinematic/desktop test` and `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml providers -- --nocapture`.

Expected: command/API tests pass without exposing credential material.

- [ ] **Step 5: Commit command surface**

Run: `git add apps/desktop/src-tauri/src/providers apps/desktop/src-tauri/src/lib.rs packages/domain/src/workflow.ts apps/desktop/src/features/workflows/api.ts && git commit -m "feat: expose provider configuration commands"`

### Task 8: Add accessible provider settings and execution status UI

**Files:**
- Create: `apps/desktop/src/features/providers/ProviderSettings.tsx`
- Create: `apps/desktop/src/features/providers/ProviderSettings.test.tsx`
- Modify: `apps/desktop/src/features/workflows/WorkflowRunView.tsx`
- Modify: `apps/desktop/src/features/workflows/WorkflowWorkspace.tsx`
- Modify: `apps/desktop/src/features/workflows/format.ts`
- Modify: `apps/desktop/src/styles/app.css`

**Interfaces:**
- Provider setup shows provider availability, masked credential state, model selection, capabilities, validation feedback, and replace/delete actions.
- Workflow review shows provider/model/parameters before execution when not already locked; runtime uses Queued/Submitting/Generating/Downloading result/Completed/Failed/Cancelling/Cancelled labels.
- Retry is shown only for retryable normalized errors; provider job ID is secondary technical detail.

- [ ] **Step 1: Write failing React tests**

Assert masked fields never render returned secret text, accessible labels and keyboard actions exist, normalized states render instead of raw provider statuses, and retry/cancel controls follow backend state.

- [ ] **Step 2: Run focused UI tests and verify they fail**

Run: `pnpm --filter @cinematic/desktop test -- src/features/providers/ProviderSettings.test.tsx src/features/workflows/WorkflowRunView.test.tsx`.

Expected: missing provider settings and status UI assertions.

- [ ] **Step 3: Implement the minimal accessible UI**

Reuse P3 focus restoration, error description, button, details, and reduced-motion conventions. Do not put credential values in React state after submit or in console/toast strings.

- [ ] **Step 4: Run all frontend tests and production build**

Run: `pnpm --filter @cinematic/desktop test` and `pnpm --filter @cinematic/desktop build`.

Expected: all frontend tests pass and `apps/desktop/dist` is generated for Tauri compilation.

- [ ] **Step 5: Commit provider UI**

Run: `git add apps/desktop/src/features/providers apps/desktop/src/features/workflows apps/desktop/src/styles/app.css && git commit -m "feat: add provider setup and execution status UI"`

### Task 9: Acceptance hardening and final verification

**Files:**
- Modify: `apps/desktop/src-tauri/tests/provider_acceptance.rs`
- Modify: `apps/desktop/src-tauri/src/providers/tests.rs`
- Modify: `README.md` only if new local verification commands are needed
- Test: full repository

- [ ] **Step 1: Add missing acceptance matrix cases**

Ensure the suite explicitly covers contract success/error normalization, approval gate, locked snapshot immutability, durable artifact linkage, all six crash windows, cancellation variants, retry attempt history, concurrency isolation, credential redaction, DryRun no-network behavior, and malformed provider responses.

- [ ] **Step 2: Run the complete verification set**

Run: `pnpm test`, `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`, `pnpm --filter @cinematic/desktop build`, `cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml`, and `git diff --check`.

Expected: all commands exit 0; the Tauri build sees the frontend `dist` directory; no diff-check whitespace errors appear.

- [ ] **Step 3: Review the diff against the spec checklist**

Confirm provider logic is absent from Canon/skill definitions, compiled request serialization is unchanged, attempts/jobs/artifacts are durable and idempotent, secrets are backend-only, and no automatic fallback or hidden workflow state machine exists.

- [ ] **Step 4: Commit final hardening**

Run: `git add apps/desktop/src-tauri/tests apps/desktop/src-tauri/src/providers README.md && git commit -m "test: harden provider integration acceptance"`

- [ ] **Step 5: Verify branch state for handoff**

Run: `git status --short --branch` and `git log --oneline -10`.

Expected: branch is `codex/provider-integrations`; only intentionally generated/local files remain untracked or ignored, and the implementation commits are visible.

