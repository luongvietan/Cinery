# P10.2 Completion: Zero-Warning Clippy Gate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `cargo clippy --all-targets -- -D warnings` pass with zero warnings without `#[allow]` suppression, by (a) Box-ing the provider error, (b) introducing parameter/context structs for the few high-radius multi-argument APIs, and (c) fixing every mechanical and semantic lint.

**Architecture:** The user chose full refactor (option 1) over `#[allow]` suppression. The provider error path is the only large blast radius: `ProviderError` (7 fields ≈ 9 words on x64) exceeds Clippy's `result_large_err` threshold (128 bytes) in 23+ `Result` signatures across `adapter.rs`, `credential_store.rs`, `declarative.rs`, and `registry.rs`. The fix is to `Box` the error body and expose it through accessor methods plus a `take_error` escape hatch, keeping every call-site `?` working unchanged. The remaining warnings are ~30 `too_many_arguments`/`type_complexity` on a small set of repository/service/storage functions (grouped into parameter structs) and ~100 mechanical or test-only lints (imports, borrows, closures, `mut`, dead helpers, collapsed ifs, etc.).

**Tech Stack:** Rust 1.77.2 workspace at `apps/desktop/src-tauri` (`cinematic-desktop` crate).

**Spec:** Agreed in-session: full refactor to satisfy `cargo clippy --all-targets -- -D warnings`; no new `#[allow(...)]` attributes; no `clippy.toml` threshold changes.

## Global Constraints

- Gate: `cargo clippy --all-targets -- -D warnings` must exit 0 at the end of every slice except where noted (Slice 1 ends clean for mechanical lints only if slices 2-4 land together; treat the full gate as end-state only).
- No new `#[allow(clippy::...)]`, no `#[expect]`, no `clippy.toml`, no lint reconfiguration.
- No behavioral change: all existing tests must pass unchanged (except dead test helpers that are deleted outright).
- Public API changes are crate-internal (single `lib` + binary + tests); there are no external consumers.
- Never touch secret redaction logic semantics (`redact_secret`), attempt/job lifecycle, replay-safety, or promotion CAS behavior.
- Rust edition 2021: `Box<ProviderError>` still implements `Deref`, so `err.kind` field access becomes `err.kind()` method or explicit match — keep call sites compiling with minimal diff by centralizing access.

---

## File Responsibility Map

- `apps/desktop/src-tauri/src/providers/error.rs`: `ProviderError` gains a boxed storage strategy, accessors, and constructors preserved.
- `apps/desktop/src-tauri/src/providers/adapter.rs`: trait signatures stay `Result<_, ProviderError>`; the thin struct is now small enough for `result_large_err`.
- `apps/desktop/src-tauri/src/providers/{declarative,credential_store,registry,service,mock,fake_async,dry_run,llm}.rs`: call sites compile via accessors/match arms.
- `apps/desktop/src-tauri/src/providers/repository.rs`: `create_attempt` takes `NewExecutionAttempt` struct.
- `apps/desktop/src-tauri/src/providers/service.rs`: `submit_*` takes `ProviderSubmitRequest` struct; `resolve_openai_token`/closure cleanup; unused imports.
- `apps/desktop/src-tauri/src/workflow/repository.rs`: `create_run*` take `NewWorkflowRun` struct.
- `apps/desktop/src-tauri/src/workflow/runtime.rs`: `create_shot_i2v_run` takes a run-creation context struct; mechanical lints.
- `apps/desktop/src-tauri/src/workflow/tbd_policy.rs`: test helper struct-literal test fixes.
- `apps/desktop/src-tauri/src/generation/storage.rs`: `materialize_media` takes `MaterializeMediaRequest`.
- `apps/desktop/src-tauri/src/providers/cancellation.rs`: token map type alias + `unused_mut`.
- `apps/desktop/src-tauri/src/db/migrations.rs`: dead helpers removed.
- `apps/desktop/src-tauri/tests/*.rs`: `&PathBuf`→`&Path`, unused imports/vars, `bool_comparison`, `type_complexity` alias.
- All remaining files listed in the per-task lint tables.

---

### Task 1: Mechanical lints — imports, borrows, closures, mut, misc

**Files:**
- Modify: `src/providers/declarative.rs:6,1267`, `src/providers/service.rs:10-11`, `src/workflow/ingestion.rs:41`, `tests/provider_keychain_acceptance.rs:16`, `tests/reference_attachment_acceptance.rs:5`, `tests/unified_scene_golden_path.rs:23`, `tests/video_generation_golden_path.rs:19`
- Modify: `src/db/migrations.rs:956-1001` (needless_borrow ×8), `src/workflow/repository.rs:194`, `src/skills/registry.rs:457`, `tests/privacy_integration.rs:40`
- Modify: `src/diagnostics/redaction.rs:25`, `src/providers/service.rs:925`, `src/workflow/runtime.rs:1444,1446,1873,1875`
- Modify: `src/db/migrations.rs:1061`, `src/providers/cancellation.rs:28`
- Modify: `src/db/migrations.rs:849-1024` (dead helpers), `tests/cinema_tbd.rs:27`
- Modify: `src/diagnostics/bundle.rs:149`, `src/qa/workflow.rs:171,199`, `src/integration/provenance.rs:590`, `src/worlds/service.rs:188`, `src/workflow/compiler.rs:753`, `src/providers/config.rs:1322`, `src/recovery/service.rs:174,197,201`, `src/workflow/completion.rs:229`, `src/workflow/context.rs:1452`, `src/canon/model.rs:27`, `src/providers/model.rs:292`, `tests/support/mod.rs:48`
- Modify: `src/scenes/service.rs:3269`, `src/workflow/runtime.rs:3214,3610,3960`, `tests/cinema_service.rs:119`, `tests/video_generation_golden_path.rs:44`
- Test: `cargo clippy --all-targets 2>&1 | Select-String 'clippy::'` shows none of: `unused_imports`, `needless_borrow*`, `redundant_closure`, `unused_mut`, `dead_code`, `needless_question_mark`, `manual_inspect`, `single_match`, `collapsible_str_replace`, `extend_with_drain`, `field_reassign_with_default`, `if_same_then_else`, `redundant_guards`, `redundant_field_names`, `derivable_impls`, `doc_lazy_continuation`, `unused_variables`, `items_after_test_module`, `bool_assert_comparison`, `manual_range_contains`

**Interfaces:**
- Consumes: the warning location table captured from `cargo clippy --all-targets --message-format=json`.
- Produces: warnings reduced from 132 to the 3 structural groups (result_large_err, too_many_arguments, type_complexity, ptr_arg).

- [x] **Step 1: Unused imports — delete the `use` lines**
  - `src/providers/declarative.rs:6` (`AuthConfig`), `:1267` (`std::time::Duration` — note: this is inside `#[cfg(test)]`? verify with the warning, delete only the flagged one)
  - `src/providers/service.rs:10-11` (`HttpRequest`, `HttpResponse`, `CustomProviderPurpose`)
  - `src/workflow/ingestion.rs:41` (`std::time::Duration`)
  - Tests: `provider_keychain_acceptance.rs:16`, `reference_attachment_acceptance.rs:5`, `unified_scene_golden_path.rs:23` (`tempdir`), `video_generation_golden_path.rs:19` (`OptionalExtension`)

- [x] **Step 2: needless_borrow / needless_borrows_for_generic_args — remove `&`**
  - `src/db/migrations.rs` lines 956-958, 977-980, 1001
  - `src/workflow/repository.rs:194`
  - `src/providers/service.rs` — not applicable here; see Task 3
  - `src/skills/registry.rs:457`, `tests/privacy_integration.rs:40`

- [x] **Step 3: redundant_closure — inline the closure**
  - `src/diagnostics/redaction.rs:25`, `src/providers/service.rs:925`, `src/workflow/runtime.rs:1444,1446,1873,1875`

- [x] **Step 4: unused_mut — drop `mut`**
  - `src/db/migrations.rs:1061`, `src/providers/cancellation.rs:28`

- [x] **Step 5: dead_code — delete unused helpers**
  - `src/db/migrations.rs`: `migrate_legacy_project` (849), `insert_legacy_scene` (875), `insert_legacy_shot` (884), `insert_authoritative_scene` (894), `shot_scene` (903), `video_ready_conn` (1024) — verify each is `#[cfg(test)]`-adjacent helper unused by any test; delete function + any now-unused private deps
  - `tests/cinema_tbd.rs:27` `canonical_version` — delete

- [x] **Step 6: one-line semantic rewrites**
  - `src/diagnostics/bundle.rs:149` `needless_question_mark`: drop `Ok(x?)` wrapper
  - `src/qa/workflow.rs:171,199` `manual_inspect`: `.map_err(|e| { log…; e })` → `.inspect_err(...)`
  - `src/integration/provenance.rs:590` `single_match`: `match x { A => …, _ => … }` → `if`
  - `src/worlds/service.rs:188` `collapsible_str_replace`: chain two `replace` calls
  - `src/workflow/compiler.rs:753` `extend_with_drain`: `extend(drain(..))` → `append`
  - `src/providers/config.rs:1322` `field_reassign_with_default`: struct literal init
  - `src/recovery/service.rs:174` `if_same_then_else`, `:197,201` `redundant_guards`
  - `src/workflow/completion.rs:229` `if_same_then_else`
  - `src/workflow/context.rs:1452` `redundant_field_names`
  - `src/canon/model.rs:27` `should_implement_trait`: rename `from_str` → `parse` (or implement `FromStr`); prefer `parse` to avoid trait semantics
  - `src/providers/model.rs:292` `derivable_impls`: `#[derive(Default)]` and delete manual impl (only if it is truly `Default::default()`-equivalent; otherwise add missing field to keep behavior — do NOT `allow`)
  - `tests/support/mod.rs:48` `doc_lazy_continuation`: indent continuation lines
  - `src/scenes/service.rs:3269` `unused variable: v1`; `src/workflow/runtime.rs:3214,3610,3960` unused vars `project_id`, `world_id`, `waiting`; `tests/cinema_service.rs:119` `look`; `tests/video_generation_golden_path.rs:44` `compilation` — delete or prefix `_` only when the binding is genuinely read later (prefer deletion)
  - `src/workflow/runtime.rs:4052` `bool_assert_comparison`: `assert_eq!(x, true)` → `assert!(x)`
  - `src/workflow/runtime.rs:3448` `manual_range_contains`
  - `tests/reference_attachment_acceptance.rs:176` `bool_comparison`: `assert_eq!(x, false)` → `assert!(!x)`

- [x] **Step 7: items_after_test_module**
  - `src/providers/service.rs:1304`, `src/workflow/context.rs:1484`, `src/workflow/ingestion.rs:2` — move the trailing items above `#[cfg(test)] mod tests` or into it

- [x] **Step 8: Verify slice**
  - `cargo clippy --all-targets 2>&1 | Select-String 'clippy::'` — expect only `result_large_err`, `too_many_arguments`, `type_complexity`, `ptr_arg`, `cloned_ref_to_slice_refs`, `collapsible_if`, `result_large_err` groups remain
  - `cargo test --all-targets` subset: `cargo test --lib` passes

- [x] **Step 9: Commit** `refactor: clear mechanical clippy lints`

### Task 2: cloned_ref_to_slice_refs + collapsible_if + ptr_arg

**Files:**
- Modify: `src/workflow/context.rs:729`, `src/workflow/prerequisites.rs:294`, `src/workflow/tbd_policy.rs:316,349,397` (`cloned_ref_to_slice_refs` — remove `.clone()` where `&[_]` expected)
- Modify: `src/scenes/service.rs:1381,1523,1635` (`collapsible_if`)
- Modify: `src/cinema/promotion.rs:691`, `tests/cinema_repository.rs:16`, `tests/cinema_service.rs:36`, `tests/cinema_tbd.rs:80`, `tests/cinema_workspace_crud.rs:21` (`ptr_arg`: `&PathBuf` → `&Path`, add `use std::path::Path` where needed)
- Test: none of `cloned_ref_to_slice_refs`, `collapsible_if`, `ptr_arg` remain

**Interfaces:**
- Produces: only `result_large_err`, `too_many_arguments`, `type_complexity` remain after both Tasks 1-2.

- [x] **Step 1: cloned_ref_to_slice_refs** — at each site the pattern is `&vec.clone()[..]`-style; replace with `&vec[..]`. Read each call site first; confirm the clone isn't needed for ownership (it isn't — clippy proves the compiler reborrows).
- [x] **Step 2: collapsible_if** — collapse `if a { if b { … } }` into `if a && b { … }`.
- [x] **Step 3: ptr_arg** — change signatures and internal `path.as_path()`/`display()` calls as needed.
- [x] **Step 4: Verify + commit** — `refactor: clear ptr_arg and collapsible lints`

### Task 3: ProviderError boxing (result_large_err)

**Files:**
- Modify: `src/providers/error.rs` (core change)
- Modify: `src/providers/adapter.rs`, `registry.rs`, `credential_store.rs`, `declarative.rs` (compile fixes)
- Test: `src/providers/error.rs` unit tests + `cargo test --lib providers`

**Interfaces:**
- Produces: `ProviderError` becomes a small handle struct wrapping `Box<ProviderErrorBody>`; size drops to 2 words; every `Result<_, ProviderError>` signature stays.
- Consumes: all 169 `ProviderError` references compile unchanged via `Deref`-based accessors.

- [x] **Step 1: Add failing size test**
  ```rust
  #[test]
  fn provider_error_is_small_enough_for_result_large_err() {
      assert!(std::mem::size_of::<ProviderError>() <= 16);
  }
  ```
  (Clippy's threshold is 128 bytes; 16 asserts the boxed handle layout.)

- [x] **Step 1b: Add accessor-parity tests** — for every public constructor/method used across the crate (`new`, `with_diagnostic`, `with_status_code`, `with_provider_message`, `with_request_id`, `with_operation`, `display_text`), assert behavior unchanged after the boxing.

- [x] **Step 2: Introduce boxed body**
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "camelCase")]
  pub struct ProviderError(Box<ProviderErrorBody>);

  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "camelCase")]
  pub struct ProviderErrorBody {
      pub kind: ProviderErrorKind,
      pub message: String,
      pub diagnostic: Option<String>,
      pub status_code: Option<u16>,
      pub provider_message: Option<String>,
      pub request_id: Option<String>,
      pub operation: Option<String>,
  }

  impl std::ops::Deref for ProviderError {
      type Target = ProviderErrorBody;
      fn deref(&self) -> &ProviderErrorBody { &self.0 }
  }
  ```
  Serialization shape is unchanged (`#[serde(transparent)]`-like behavior via newtype over `Box<T>` is automatic because `Box<T: Serialize>` serializes as `T`). **Verify serde shape with a test** comparing `serde_json::to_value` before/after keys.

- [x] **Step 3: Keep the API ergonomic**
  - `pub const fn kind(&self) -> &ProviderErrorKind { &self.0.kind }` — wait, `&self.0.kind` borrows through `Deref`; just `&self.0.kind` works. Add `pub fn kind(&self) -> &ProviderErrorKind`.
  - Where callers pattern-matched `match err.kind { … }` they now need `match err.kind()` — fix all such sites in `declarative.rs`, `service.rs`, `workflow/background_failures.rs`, `workflow/completion.rs`, `qa/*` (search `\.kind` on `ProviderError` values).
  - `display_text` moves to `impl ProviderError` delegating to body.

- [x] **Step 4: Fix compile errors crate-wide**
  - `grep -rn "\.kind\b" src/providers src/workflow | grep -i provider` and adjust to `kind()` or keep field-style via Deref auto-deref on `err.kind` (field access auto-derefs! `err.kind` still compiles through `Deref`). Prefer keeping `err.kind` working via Deref field access — no call-site change needed for reads.
  - Constructor sites `ProviderError::new(...)` keep signature; builder methods mutate `self.0` via `DerefMut`:
    ```rust
    pub fn with_diagnostic(mut self, diagnostic: impl Into<String>) -> Self {
        self.0.diagnostic = Some(redact_secret(&diagnostic.into()));
        self
    }
    ```

- [x] **Step 5: Handle serde transparent check** — newtype `ProviderError(Box<Body>)` serializes as the inner body automatically (Box<T> is transparent for serde). Add the serialization-parity test in Step 1b.

- [x] **Step 6: Verify**
  - `cargo test --lib providers` passes; serialization-parity and size tests pass
  - `cargo clippy --all-targets 2>&1 | Select-String 'result_large_err'` — empty
  - `cargo clippy --all-targets 2>&1 | Select-String 'clippy::' | Measure-Object | % Count` — only `too_many_arguments` + `type_complexity` remain (about 10)

- [x] **Step 7: Commit** `refactor: box ProviderError to satisfy result_large_err`

### Task 4: Parameter objects for too_many_arguments + type_complexity

**Files:**
- Modify: `src/workflow/repository.rs:14,47` — `create_run` / `create_run_in_transaction` take `NewWorkflowRun<'a>` struct; internal call `runtime.rs:147` `create_shot_i2v_run` (8 args) refactored to consume the struct + operation/report
- Modify: `src/providers/repository.rs:461` — `create_attempt` takes `NewExecutionAttempt<'a>`
- Modify: `src/providers/service.rs:853,878` — `submit_prepared_request` / `submit_provider_request` take `ProviderSubmitRequest<'a>` struct
- Modify: `src/generation/storage.rs:48` — `materialize_media` takes `MaterializeMediaRequest<'a>`
- Modify: `src/workflow/tbd_policy.rs:254` — test helper `test_tbd` (8 args, test-only) → struct literal or smaller helper
- Modify: `src/providers/cancellation.rs:10`, `src/providers/service.rs:1320` — `type` aliases for the complex types
- Modify: `tests/world_scene_pipeline_acceptance.rs:954` — type alias in test
- Test: none of `too_many_arguments` / `type_complexity` remain

**Interfaces:**
- Produces: `NewWorkflowRun<'a> { project_id, skill_id, skill_version, operation_id, input, prerequisite_report, steps }`, `NewExecutionAttempt<'a> { workflow_run_id, step_definition_id, attempt_number, compiled_request_id, provider_id, model_id, idempotency_key }`, `ProviderSubmitRequest<'a> { request, reference_attachments, project_root, credentials, step_id, compiled_request_id, provider_id, model_id, attempt_number }`, `MaterializeMediaRequest<'a> { project_root, workflow_run_id, provider_attempt_id, ordinal, bytes, mime_type, extension, width, height }`.
- Consumes: callers in `runtime.rs`, `providers/service.rs`, `cinema/promotion.rs`, `generation/service.rs`, `assets/*`.

- [x] **Step 1: Failing-size compile-proof via tests-first is not meaningful for signatures; instead write a call-site inventory test**: none — proceed directly; the gate is the lint itself.
- [x] **Step 2: Define structs adjacent to each function; keep field order = old argument order.**
- [x] **Step 3: Migrate call sites mechanically.** Inventory via grep before editing:
  - `create_run` / `create_run_in_transaction`: `rg -n "create_run" src apps`
  - `create_attempt`: `rg -n "create_attempt" src tests`
  - `submit_provider_request` / `submit_prepared_request`: `rg -n "submit_provider_request|submit_prepared_request" src tests`
  - `materialize_media`: `rg -n "materialize_media" src tests`
  - `create_shot_i2v_run`: `rg -n "create_shot_i2v_run" src`
- [x] **Step 4: `test_tbd` helper** — replace with a `CanonTbdRecord { id, …, ..base() }` pattern or pass a struct; test-only, keep call sites in the same file.
- [x] **Step 5: Type aliases**
  - `src/providers/cancellation.rs:10`: name it `type TokenMap = …` (read actual type first)
  - `src/providers/service.rs:1320`: alias for the closure/stream type (read actual type first)
  - `tests/world_scene_pipeline_acceptance.rs:954`: alias in test file
- [x] **Step 6: Verify + commit** `refactor: group multi-arg APIs into request structs`

### Task 5: Full gate + regression

- [x] **Step 1:** `cargo clippy --all-targets -- -D warnings` exits 0
- [x] **Step 2:** `cargo fmt --all -- --check` clean (run `cargo fmt --all` first if needed)
- [x] **Step 3:** `cargo test` (all targets) passes
- [x] **Step 4:** Frontend untouched — no pnpm/test run required, but if any `packages/domain` or `apps/desktop/src` file changed, run `pnpm test`
- [x] **Step 5:** Commit `gate: enforce zero-warning clippy across all targets`
- [x] **Step 6:** Update this file's checkboxes; report lint-before/after (132 → 0) and the two structural refactors (boxed error, request structs) in the final summary

## Verification Commands

```powershell
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
cargo test
```

(Run from `apps/desktop/src-tauri`. Frontend gates only if frontend files changed.)

## Rollback

Each task is an independent commit; revert per task if a slice regresses behavior that tests catch.
