# P10.2 Shot-Scoped Image-to-Video Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Generate a durable, reviewable video candidate from the exact keyframe version pinned to a Shot and explicitly promote one exact candidate version back to that Shot.

**Architecture:** Add a dedicated `shot.image_to_video` operation to the existing `scene-builder` workflow. Freeze the source `AssetVersion` in workflow input at run creation, compile it into `ExecutionRequest.references`, route through the P10.1 provider attempt/job/background runner, capture into existing result/lineage and scene video asset structures, then expose a conflict-safe Shot promotion command and Shot-local UI.

**Tech Stack:** Rust 1.77.2, Tauri 2, SQLite/rusqlite, serde, React 18, TypeScript 5.8, Vitest 3, Testing Library, pnpm 9.12.3.

**Spec:** `docs/superpowers/specs/2026-08-31-p10-2-shot-scoped-image-to-video-design.md`

## Global Constraints

- Baseline implementation starts from `master@d55179d`; design commit `d7ecdf3` is the only later baseline commit.
- Do not redesign or replace P10.1 `ProviderJob`, `BackgroundJobRunner`, attempts, completion ordering, retry, cancellation, or restart behavior.
- Never resolve `Asset -> latest version`; capture `sourceAssetVersionId` atomically when creating the run and use that id thereafter.
- `ExecutionRequest` remains provider-neutral; provider/model selection remains execution metadata.
- Provider and model routing must use `supports_image_to_video` / `video.imageToVideo`, never names or substrings.
- Generated output remains a candidate until a human explicitly promotes it.
- Completion and promotion must be replay-safe; no duplicate result sets, artifacts, versions, lineage, or remote submissions.
- Existing `scene.generate_video`, keyframe generation, image generation, sync/async providers, retry, cancellation, WorkflowRunView, JobsPanel, and Cinema behavior must remain compatible.
- Do not add migration `0023` unless existing immutable workflow input, lineage, promotions, Shot pins, provider jobs, and audit events prove insufficient; stop for design review before adding schema.
- Reuse the existing visual system and components. Do not add a standalone developer screen, unrelated UI rewrite, or new polling timer.
- Manual GUI and clean-install gates remain open unless they are actually executed.
- All production code is written only after a focused test has failed for the expected missing behavior.

---

## File Responsibility Map

- `apps/desktop/src-tauri/src/workflow/execution.rs`: provider-neutral task, source-image role, and immutable generation parameter schema.
- `apps/desktop/src-tauri/src/workflow/model.rs`: pinned asset snapshot status.
- `apps/desktop/src-tauri/src/workflow/repository.rs`: reusable in-transaction run insertion and active logical-run lookup.
- `apps/desktop/src-tauri/src/workflow/context.rs`: exact persisted keyframe validation and immutable Shot I2V context.
- `apps/desktop/src-tauri/src/workflow/compiler.rs`: `shot_image_to_video_v1` compiler.
- `apps/desktop/src-tauri/src/workflow/runtime.rs`: atomic input freezing/deduplication, resolver/compiler dispatch, and durable I2V execution routing.
- `apps/desktop/src-tauri/src/skills/builtin/scene_builder.rs`: new workflow operation definition.
- `apps/desktop/src-tauri/src/skills/registry.rs`: operation/input/compiler validation allowlists.
- `apps/desktop/src-tauri/src/providers/model.rs`: request mapping and strict capability enforcement.
- `apps/desktop/src-tauri/src/providers/declarative.rs`: exact `video.imageToVideo` selection without fallback.
- `apps/desktop/src-tauri/src/workflow/completion.rs`: one-result video candidate import for Shot I2V.
- `apps/desktop/src-tauri/src/cinema/promotion.rs`: Shot-specific candidate validation, idempotent artifact promotion, compare-and-set pin, and audit event.
- `apps/desktop/src-tauri/src/cinema/{model,repository,commands,mod}.rs`: promotion contract, CAS repository primitive, and Tauri command.
- `apps/desktop/src-tauri/src/cinema/service.rs`: exact keyframe source projection used only to render the Shot input preview.
- `packages/domain/src/{execution,workflow,cinema}.ts`: frontend mirrors for additive request/snapshot/promotion types.
- `apps/desktop/src/features/providers/ProviderModelFields.tsx`: operation-capability and model-capability filtering.
- `apps/desktop/src/features/scenes/ShotImageToVideo.tsx`: Shot-local input, durable status, result review, restoration, and promotion UI.
- `apps/desktop/src/features/scenes/SceneShots.tsx`: mount the Shot I2V panel in the existing Shots workspace.
- `apps/desktop/src/features/scenes/api.ts` and `apps/desktop/src/features/workflows/api.ts`: command bindings.
- `apps/desktop/src/test/StatefulTauriFacade.ts`: deterministic frontend command facade.
- `apps/desktop/src-tauri/tests/background_video_job_acceptance.rs`: declarative I2V cold-adapter/restart/cancel/retry coverage.
- `apps/desktop/src-tauri/tests/shot_image_to_video_golden_path.rs`: complete Shot -> keyframe -> candidate -> promotion chain.

---

### Task 1: Freeze the Exact Shot Keyframe and Compile the I2V Request

**Files:**
- Modify: `apps/desktop/src-tauri/src/error.rs:9-291`
- Modify: `apps/desktop/src-tauri/src/workflow/model.rs:95-99,202-243`
- Modify: `apps/desktop/src-tauri/src/workflow/execution.rs:102-192`
- Modify: `apps/desktop/src-tauri/src/workflow/repository.rs:14-79`
- Modify: `apps/desktop/src-tauri/src/workflow/context.rs:797-855,1824-1871`
- Modify: `apps/desktop/src-tauri/src/workflow/compiler.rs:286-381`
- Modify: `apps/desktop/src-tauri/src/workflow/runtime.rs:54-127,140-533,2708-2910`
- Modify: `apps/desktop/src-tauri/src/skills/builtin/scene_builder.rs:7-107`
- Modify: `apps/desktop/src-tauri/src/skills/registry.rs:133-309`
- Modify: `packages/domain/src/execution.ts:1-101`
- Modify: `packages/domain/src/workflow.ts:73-95`
- Test: module tests in the Rust files above

**Interfaces:**
- Consumes: existing `ShotRecord`, `WorkflowContextSnapshot`, `ExecutionRequest`, `WorkflowRepository::create_run`, and `scene-builder` conventions.
- Produces: `ExecutionTask::ShotImageToVideo`, `ReferenceRole::SourceImage`, `ExecutionGenerationParameters`, `AssetSnapshotStatus::Pinned`, `resolve_shot_image_to_video_context`, `ShotImageToVideoCompiler`, and persisted input containing `sourceAssetVersionId`.

- [ ] **Step 1: Add failing backward-compatibility and request-schema tests**

```rust
#[test]
fn old_execution_request_defaults_generation_parameters() {
    let request: ExecutionRequest = serde_json::from_value(serde_json::json!({
        "requestVersion": 1,
        "task": "scene_video",
        "mediaType": "video",
        "prompt": "move",
        "references": [],
        "constraints": [],
        "expectedOutput": test_video_output(),
        "provenance": test_provenance()
    })).unwrap();
    assert_eq!(request.generation_parameters, ExecutionGenerationParameters::default());
}

#[test]
fn shot_i2v_request_preserves_source_role_and_parameters() {
    let request = shot_i2v_request("version-exact");
    let value = serde_json::to_value(&request).unwrap();
    assert_eq!(value["task"], "shot_image_to_video");
    assert_eq!(value["references"][0]["role"], "source_image");
    assert_eq!(value["generationParameters"]["durationSeconds"], 4.0);
}
```

- [ ] **Step 2: Run schema tests and verify RED**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml old_execution_request_defaults_generation_parameters
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml shot_i2v_request_preserves_source_role_and_parameters
```

Expected: compilation fails because the new types and field do not exist.

- [ ] **Step 3: Add provider-neutral additive types**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionTask {
    CharacterFaceLock,
    CharacterOutfit,
    CharacterSheet,
    WorldPlate,
    ShotKeyframe,
    SceneVideo,
    ShotImageToVideo,
    VisualRepair,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceRole {
    World,
    CharacterLook,
    CharacterSheet,
    Prop,
    SourceImage,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionGenerationParameters {
    pub aspect_ratio: Option<String>,
    pub duration_seconds: Option<f32>,
    pub fps: Option<u32>,
    pub seed: Option<u64>,
}
```

Add to `ExecutionRequest`:

```rust
#[serde(default)]
pub generation_parameters: ExecutionGenerationParameters,
```

Add `Pinned` to `AssetSnapshotStatus` and mirror serialized names in TypeScript:

```ts
export interface ExecutionGenerationParameters {
  aspectRatio?: string;
  durationSeconds?: number;
  fps?: number;
  seed?: number;
}

export interface AssetSnapshotRef {
  assetId: string;
  assetVersionId: string;
  assetType: string;
  versionNumber: number;
  status: "canonical" | "pinned";
  path: string;
}
```

- [ ] **Step 4: Run schema tests and verify GREEN**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml workflow::execution::tests
pnpm --filter @cinematic/domain test
```

Expected: execution/domain tests pass and old requests remain readable.

- [ ] **Step 5: Add failing atomic-freeze and exact-context tests**

```rust
#[test]
fn shot_i2v_run_freezes_keyframe_before_context_resolution() {
    let fixture = shot_i2v_fixture();
    let run = WorkflowRuntime::create_run(
        &fixture.root,
        "scene-builder",
        "1.0.0",
        "shot.image_to_video",
        serde_json::json!({
            "sceneId": fixture.scene_id,
            "shotId": fixture.shot_id,
            "providerId": "fake_async_video",
            "modelId": "fake-video-v1",
            "prompt": "A measured push-in"
        }),
    ).unwrap();
    let frozen: serde_json::Value = serde_json::from_str(&run.run.input_json).unwrap();
    assert_eq!(frozen["sourceAssetVersionId"], fixture.first_keyframe_version_id);
    CinemaService::set_shot_keyframe(&fixture.root, &fixture.shot_id, Some(&fixture.second_keyframe_version_id)).unwrap();
    let waiting = WorkflowRuntime::advance_run(&fixture.root, &run.run.id).unwrap();
    let request: ExecutionRequest = compiled_request(&waiting);
    assert_eq!(request.references[0].reference, fixture.first_keyframe_version_id);
}
```

Add focused failures for `ShotNotFound`, `SourceKeyframeMissing`, missing/cross-project version, non-image MIME, and superseded-but-pinned image.

- [ ] **Step 6: Run context tests and verify RED**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml shot_i2v_run_freezes_keyframe_before_context_resolution
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml shot_i2v_rejects -- --nocapture
```

Expected: operation lookup or input validation fails.

- [ ] **Step 7: Refactor run insertion for shared immediate transactions**

```rust
pub(crate) fn create_run_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    project_id: &str,
    skill_id: &str,
    skill_version: &str,
    operation_id: &str,
    input: &Value,
    prerequisite_report: &PrerequisiteReport,
    steps: &[WorkflowStepDefinition],
) -> Result<String, AppError>;
```

Keep `WorkflowRepository::create_run` as a wrapper that begins `TransactionBehavior::Immediate`, calls the helper, and commits. For `shot.image_to_video`, begin the immediate transaction in `create_run_for_operation`, execute this scoped lookup, enrich input, deduplicate, then insert:

```sql
SELECT s.duration_seconds, s.keyframe_asset_version_id
FROM scene_shots s
JOIN world_scenes ws ON ws.id = s.scene_id
WHERE s.id = ?1 AND s.scene_id = ?2 AND ws.project_id = ?3
```

Persist `sourceAssetVersionId` and default `generationParameters.durationSeconds`. Return an existing active run only when operation and normalized input JSON match and status is `created`, `running`, `waiting_for_approval`, or `ready_for_execution`.

- [ ] **Step 8: Implement exact resolver, compiler, operation, and typed errors**

```rust
#[error("this Shot has no source keyframe")]
SourceKeyframeMissing,
#[error("the source keyframe is not an image")]
SourceMediaTypeUnsupported,
#[error("the selected AI service or model does not support image-to-video")]
ImageToVideoUnsupported,
#[error("the Shot video changed before promotion completed")]
PromotionConflict,
```

```rust
pub fn resolve_shot_image_to_video_context(
    conn: &Connection,
    project_id: &str,
    skill_id: &str,
    skill_version: &str,
    operation_id: &str,
    input: &Value,
    prerequisite_report: PrerequisiteReport,
) -> Result<WorkflowContextSnapshot, AppError>;

pub struct ShotImageToVideoCompiler;
```

The resolver reads only `input.sourceAssetVersionId`, validates exact version/project/type/MIME/path, and adds one pinned asset snapshot. The compiler emits one `SourceImage` reference, task `ShotImageToVideo`, media type `Video`, persisted prompt/parameters, and candidate video expected output owned by `sceneId`. Register workflow steps `validate-input`, `resolve-context`, `compile-request`, `approve-request`, `execute`, `complete`.

- [ ] **Step 9: Run exact-input/compiler tests and verify GREEN**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml shot_i2v -- --nocapture
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml workflow::repository::tests
```

Expected: exact input, typed validation, superseded pin, immutable request, and active-run dedupe tests pass.

- [ ] **Step 10: Commit the exact-input slice**

```powershell
git add apps/desktop/src-tauri/src/error.rs apps/desktop/src-tauri/src/workflow apps/desktop/src-tauri/src/skills packages/domain/src/execution.ts packages/domain/src/workflow.ts
git commit -m "runtime: compile shot image-to-video from exact keyframe"
```

---

### Task 2: Enforce Image-to-Video Provider and Model Capabilities

**Files:**
- Modify: `apps/desktop/src-tauri/src/providers/model.rs:220-369,538-617`
- Modify: `apps/desktop/src-tauri/src/providers/declarative.rs:77-170,769-960,1749-1753`
- Modify: `apps/desktop/src-tauri/src/providers/fake_async.rs:56-94`
- Modify: `apps/desktop/src-tauri/src/providers/service.rs:758-791,1080-1106`
- Test: module tests in the files above

**Interfaces:**
- Consumes: Task 1 task/parameters/reference types and declarative operation constants.
- Produces: strict `ImageToVideoUnsupported`, mapped provider parameters, exact `video.imageToVideo` selection, and an I2V-capable deterministic test provider.

- [ ] **Step 1: Add failing provider capability tests**

```rust
#[test]
fn video_with_source_image_requires_i2v_capability() {
    let request = shot_i2v_provider_request();
    let mut capabilities = video_capabilities();
    capabilities.supports_reference_image = true;
    capabilities.supports_image_to_video = false;
    assert_eq!(capabilities.supports(&request).unwrap_err(), "image-to-video is not supported");
}

#[test]
fn declarative_i2v_does_not_fallback_to_plain_video_generation() {
    let provider = declarative_provider_with_only("video.generate");
    let error = provider.submit(&shot_i2v_provider_request()).unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::UnsupportedCapability);
}

#[test]
fn model_without_i2v_operation_returns_typed_error_before_submit() {
    let fixture = provider_fixture_with_model_capabilities(vec!["video.generate"]);
    let error = ProviderService::validate_image_to_video_selection(
        &fixture.root,
        &fixture.provider_id,
        &fixture.model_id,
    ).unwrap_err();
    assert!(matches!(error, AppError::ImageToVideoUnsupported));
    assert_eq!(fixture.submit_count(), 0);
}
```

Add a mapping assertion for aspect ratio, duration, FPS, and seed.

- [ ] **Step 2: Run provider tests and verify RED**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml video_with_source_image_requires_i2v_capability
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml declarative_i2v_does_not_fallback_to_plain_video_generation
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml model_without_i2v_operation_returns_typed_error_before_submit
```

Expected: capability accepts the request or adapter selects `video.generate`.

- [ ] **Step 3: Implement strict mapping and routing**

```rust
generation_parameters: ProviderGenerationParameters {
    width: None,
    height: None,
    aspect_ratio: request.generation_parameters.aspect_ratio.clone(),
    duration_seconds: request.generation_parameters.duration_seconds,
    fps: request.generation_parameters.fps,
    seed: request.generation_parameters.seed,
},
```

```rust
if request.task == ExecutionTask::ShotImageToVideo && !self.supports_image_to_video {
    return Err("image-to-video is not supported".into());
}
```

Delete the `video.imageToVideo -> video.generate` fallback. Keep declarative model routing exact with `self.model_supports(&request.selected_model, OPERATION_VIDEO_IMAGE_TO_VIDEO)?`. Mark only the deterministic adapter used in I2V tests as capable.

Add one shared preflight that derives from the same provider definition and model capability list used by the adapter:

```rust
pub fn validate_image_to_video_selection(
    project_root: &Path,
    provider_id: &str,
    model_id: &str,
) -> Result<(), AppError>;
```

It returns `ImageToVideoUnsupported` when provider capability is false, the model is absent, or a non-empty model capability list omits `video.imageToVideo`. Empty custom-model capability lists retain their existing “all configured provider operations” meaning.

- [ ] **Step 4: Run provider tests and verify GREEN**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml providers::model::tests
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml providers::declarative::tests
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml providers::fake_async::tests
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml model_without_i2v_operation_returns_typed_error_before_submit
```

Expected: capable providers accept source attachments; provider/model mismatches reject before submission; plain video remains green.

- [ ] **Step 5: Commit provider enforcement**

```powershell
git add apps/desktop/src-tauri/src/providers
git commit -m "providers: enforce image-to-video capability routing"
```

---

### Task 3: Reuse Durable Submission, Restart, Retry, Cancellation, and Completion

**Files:**
- Modify: `apps/desktop/src-tauri/src/workflow/runtime.rs:1640-2081,2083-2449`
- Modify: `apps/desktop/src-tauri/src/workflow/completion.rs:115-434`
- Modify: `apps/desktop/src/features/workflows/labels.ts:1-80`
- Modify: `apps/desktop/src-tauri/tests/background_video_job_acceptance.rs:1-1171`

**Interfaces:**
- Consumes: compiled I2V request, strict provider adapter, attempts/jobs/background completion.
- Produces: durable jobs with operation `video.imageToVideo`, exact attachment submission, one-output capture, and restart/cancel/retry evidence.

- [ ] **Step 1: Add a failing cold-adapter I2V acceptance test**

```rust
#[test]
fn shot_i2v_resumes_through_rehydrated_declarative_adapter_without_resubmit() {
    let fixture = support::compilable_scene();
    let source_version_id = pin_shot_keyframe(&fixture);
    let server = loopback_provider::LoopbackServer::start();
    install_i2v_provider(&fixture.root, server.url());
    let running = start_shot_i2v_run(&fixture.root, &fixture.scene.id, &fixture.shots[0].id);
    let job = durable_job(&fixture.root, &running.run.id);
    assert_eq!(job.operation.as_deref(), Some("video.imageToVideo"));
    assert_eq!(server.submit_count(), 1);
    assert_eq!(server.received_source_sha256(), sha256_for_version(&fixture.root, &source_version_id));
    background::reset_provider_cache_for_tests();
    drive_background_to_completion(&fixture.root, &running.run.id);
    assert_eq!(server.submit_count(), 1);
}
```

- [ ] **Step 2: Run cold-adapter test and verify RED**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test background_video_job_acceptance shot_i2v_resumes_through_rehydrated_declarative_adapter_without_resubmit -- --exact --nocapture
```

Expected: submission lacks attachments, operation routing fails, or completion ignores the operation.

- [ ] **Step 3: Route Shot I2V through the durable video executor**

```rust
if matches!(operation.id.as_str(), "scene.generate_video" | "shot.image_to_video") {
    return execute_scene_video_ready(conn, project_root, project_id, detail, operation);
}
```

Before creating an attempt for `ShotImageToVideo`, call:

```rust
ProviderService::validate_image_to_video_selection(project_root, &provider_id, &model_id)?;
```

This must run before `create_attempt` or any provider submission so unsupported selections surface as typed product errors with zero side effects.

Resolve attachments and use the full submission API:

```rust
let attachments = resolve_reference_attachments(project_root, &request)?;
let submission = ProviderService::submit_provider_request(
    &request,
    attachments,
    Some(project_root),
    None,
    execute_step_id,
    &compiled_hash,
    &provider_id,
    &model_id,
    attempt_number,
)?;
```

Retain order: create attempt, submit, persist job operation, return for async work.

- [ ] **Step 4: Recognize Shot I2V in shared completion**

```rust
let requested_output_count = if matches!(
    context.operation_id.as_str(),
    "scene.generate_video" | "shot.image_to_video"
) { 1 } else if context.operation_id.starts_with("scene.") { 1 } else { 4 };

if matches!(context.operation_id.as_str(), "scene.generate_video" | "shot.image_to_video") {
    import_scene_video_candidate(project_root, &conn, &attempt, &context, &captured.artifacts[0])?;
}
```

Do not call `set_shot_video` during completion.

- [ ] **Step 5: Add retry/cancellation assertions and run RED then GREEN**

Assert retry keeps the first compiled reference and receives attempt 2/key suffix `:2`; cancellation resolves through the runner and creates no result set.

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test background_video_job_acceptance shot_i2v_retry_preserves_exact_source -- --exact --nocapture
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test background_video_job_acceptance shot_i2v_cancellation_is_truthful_and_terminal_safe -- --exact --nocapture
```

- [ ] **Step 6: Run P10.1 and scene-video regressions**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test background_video_job_acceptance
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test video_generation_golden_path
```

Expected: all original and I2V tests pass.

- [ ] **Step 7: Commit durable runtime integration**

```powershell
git add apps/desktop/src-tauri/src/workflow apps/desktop/src-tauri/tests/background_video_job_acceptance.rs apps/desktop/src/features/workflows/labels.ts
git commit -m "runtime: complete shot image-to-video in background"
```

---

### Task 4: Add Idempotent, Conflict-Safe Shot Promotion

**Files:**
- Create: `apps/desktop/src-tauri/src/cinema/promotion.rs`
- Modify: `apps/desktop/src-tauri/src/cinema/mod.rs:1-7`
- Modify: `apps/desktop/src-tauri/src/cinema/model.rs:17-36`
- Modify: `apps/desktop/src-tauri/src/cinema/repository.rs:305-354`
- Modify: `apps/desktop/src-tauri/src/cinema/commands.rs:125-152`
- Modify: `apps/desktop/src-tauri/src/lib.rs:20-131`
- Modify: `packages/domain/src/cinema.ts:93-125`
- Modify: `apps/desktop/src/features/scenes/api.ts:204-319`
- Modify: `apps/desktop/src/test/StatefulTauriFacade.ts:113-370`
- Test: module tests in `cinema/promotion.rs`
- Test: `apps/desktop/src-tauri/tests/cinema_commands_crud.rs`

**Interfaces:**
- Consumes: captured artifact/lineage, `GenerationService::promote_generated_artifact`, scene video asset, Shot pins, audit events.
- Produces: `promote_shot_video_candidate` command and `ShotVideoPromotionResult`.

- [ ] **Step 1: Add failing promotion tests**

```rust
#[test]
fn promotes_exact_i2v_candidate_and_keeps_source_keyframe() {
    let fixture = completed_shot_i2v_fixture();
    let before = fixture.shot();
    let promoted = promote_shot_video_candidate(
        &fixture.root,
        &fixture.shot_id,
        &fixture.artifact_id,
        before.generated_video_asset_version_id.as_deref(),
    ).unwrap();
    let after = fixture.shot();
    assert_eq!(after.keyframe_asset_version_id, before.keyframe_asset_version_id);
    assert_eq!(after.generated_video_asset_version_id.as_deref(), Some(promoted.asset_version_id.as_str()));
}

#[test]
fn conflicting_shot_promotion_is_rejected_without_repinning() {
    let fixture = completed_two_candidate_fixture();
    let first = fixture.promote_first(None);
    let error = fixture.promote_second(None).unwrap_err();
    assert!(matches!(error, AppError::PromotionConflict));
    assert_eq!(fixture.shot().generated_video_asset_version_id, Some(first.asset_version_id));
}
```

Add wrong Shot/operation, missing lineage, non-video, duplicate, and audit tests.

- [ ] **Step 2: Run promotion tests and verify RED**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml cinema::promotion::tests -- --nocapture
```

Expected: module/function/types do not exist.

- [ ] **Step 3: Add promotion contract and CAS primitive**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShotVideoPromotionResult {
    pub shot_id: String,
    pub artifact_id: String,
    pub asset_version_id: String,
    pub previous_asset_version_id: Option<String>,
}

pub fn set_shot_video_if_current(
    tx: &rusqlite::Transaction<'_>,
    project_id: &str,
    shot_id: &str,
    expected_current: Option<&str>,
    next_version: &str,
) -> Result<bool, AppError>;
```

Use nullable compare-and-set SQL scoped through the Shot's Scene/project:

```sql
UPDATE scene_shots
SET generated_video_asset_version_id = ?1, updated_at = ?2
WHERE id = ?3
  AND scene_id IN (SELECT id FROM world_scenes WHERE project_id = ?4)
  AND ((generated_video_asset_version_id IS NULL AND ?5 IS NULL)
       OR generated_video_asset_version_id = ?5)
```

- [ ] **Step 4: Implement crash-reconcilable promotion**

```rust
pub fn promote_shot_video_candidate(
    project_root: &Path,
    shot_id: &str,
    artifact_id: &str,
    expected_current_video_asset_version_id: Option<&str>,
) -> Result<ShotVideoPromotionResult, AppError>;
```

Required order: validate artifact/lineage/project; require operation/Shot/Scene/source match and available `video/mp4`; resolve scene video asset; preflight expected pin; call idempotent `promote_generated_artifact(..., true)`; begin immediate transaction; return if already pinned; otherwise CAS; return `PromotionConflict` on zero rows; append `shot.video.promoted` only when changed with Shot/source/run/attempt/artifact/prior/output ids.

- [ ] **Step 5: Add command and frontend bindings**

```rust
#[tauri::command]
pub fn promote_shot_video_candidate(
    project_root_path: String,
    shot_id: String,
    artifact_id: String,
    expected_current_video_asset_version_id: Option<String>,
) -> Result<ShotVideoPromotionResult, AppCommandError>;
```

```ts
export interface ShotVideoPromotionResult {
  shotId: string;
  artifactId: string;
  assetVersionId: string;
  previousAssetVersionId: string | null;
}

export function promoteShotVideoCandidate(
  projectRootPath: string,
  shotId: string,
  artifactId: string,
  expectedCurrentVideoAssetVersionId: string | null,
): Promise<ShotVideoPromotionResult>;
```

Register in `lib.rs` and mirror in `StatefulTauriFacade`.

- [ ] **Step 6: Run service/command tests and verify GREEN**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml cinema::promotion::tests
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test cinema_commands_crud promote_shot_video_candidate -- --nocapture
pnpm --filter @cinematic/desktop test -- StatefulTauriFacade.test.ts
```

- [ ] **Step 7: Commit Shot promotion**

```powershell
git add apps/desktop/src-tauri/src/cinema apps/desktop/src-tauri/src/lib.rs apps/desktop/src-tauri/tests/cinema_commands_crud.rs packages/domain/src/cinema.ts apps/desktop/src/features/scenes/api.ts apps/desktop/src/test/StatefulTauriFacade.ts
git commit -m "cinema: promote exact video candidate to shot"
```

---

### Task 5: Add Capability-Filtered Shot Generation and Candidate Review UI

**Files:**
- Create: `apps/desktop/src/features/scenes/ShotImageToVideo.tsx`
- Create: `apps/desktop/src/features/scenes/ShotImageToVideo.test.tsx`
- Modify: `apps/desktop/src/features/scenes/SceneShots.tsx:1-431`
- Modify: `apps/desktop/src/features/providers/ProviderModelFields.tsx:7-156`
- Modify: `apps/desktop/src/features/providers/ProviderModelFields.test.tsx`
- Modify: `apps/desktop/src/features/workflows/api.ts:15-145`
- Modify: `apps/desktop/src/features/scenes/api.ts:204-319`
- Modify: `apps/desktop/src-tauri/src/cinema/model.rs`
- Modify: `apps/desktop/src-tauri/src/cinema/service.rs`
- Modify: `apps/desktop/src-tauri/src/cinema/commands.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `packages/domain/src/cinema.ts`
- Test: `apps/desktop/src-tauri/tests/cinema_commands_crud.rs`
- Use: `apps/desktop/src/features/generation/GenerationResultCard.tsx`
- Use: `apps/desktop/src/features/assets/paths.ts`

**Interfaces:**
- Consumes: workflow/capability/promotion APIs, `WorkflowRunView`, `GenerationResultCard`.
- Produces: `ShotImageToVideo` and `ProviderModelFields.requiredOperation`.

- [ ] **Step 1: Add failing provider/model filtering test**

```tsx
it("offers only providers and models that advertise video.imageToVideo", async () => {
  mockProvider("plain-video", { supportsImageToVideo: false }, ["plain-v1"]);
  mockCustomProvider("i2v", { supportsImageToVideo: true }, [
    { id: "text-v1", name: "Text", capabilities: ["video.generate"] },
    { id: "motion-v1", name: "Motion", capabilities: ["video.imageToVideo"] },
  ]);
  render(<ProviderModelFields {...props} mediaType="video" requiresReferences requiredOperation="video.imageToVideo" />);
  expect(await screen.findByRole("option", { name: /i2v/ })).toBeEnabled();
  expect(screen.getByRole("option", { name: /plain-video/ })).toBeDisabled();
  expect(screen.getByRole("option", { name: "motion-v1" })).toBeInTheDocument();
  expect(screen.queryByRole("option", { name: "text-v1" })).not.toBeInTheDocument();
});
```

- [ ] **Step 2: Run selector test and verify RED**

```powershell
pnpm --filter @cinematic/desktop test -- ProviderModelFields.test.tsx
```

Expected: invalid prop and non-I2V model visible.

- [ ] **Step 3: Implement operation-aware filtering**

```ts
interface ProviderModelFieldsProps {
  projectRootPath: string;
  value: ProviderModelSelection;
  mediaType: "image" | "video";
  requiresReferences: boolean;
  requiredOperation?: "video.imageToVideo";
  onChange(value: ProviderModelSelection): void;
}
```

Require `supportsImageToVideo` for the I2V operation and filter custom models with:

```ts
const models = custom?.models
  .filter((model) => !requiredOperation || model.capabilities.length === 0 || model.capabilities.includes(requiredOperation))
  .map((model) => model.id) ?? discoveredModels;
```

- [ ] **Step 4: Add a failing exact-source projection command test**

```rust
#[test]
fn shot_i2v_source_returns_the_exact_pinned_version_not_latest() {
    let fixture = shot_with_two_keyframes();
    CinemaService::set_shot_keyframe(&fixture.root, &fixture.shot_id, Some(&fixture.first_version.id)).unwrap();
    AssetService::promote_asset_version(&fixture.root, &fixture.second_version.id).unwrap();
    let source = CinemaService::get_shot_image_to_video_source(&fixture.root, &fixture.shot_id).unwrap();
    assert_eq!(source.asset_version_id, fixture.first_version.id);
    assert_eq!(source.mime_type, "image/png");
}
```

- [ ] **Step 5: Run the source projection test and verify RED**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test cinema_commands_crud shot_i2v_source_returns_the_exact_pinned_version_not_latest -- --nocapture
```

Expected: service/command/type do not exist.

- [ ] **Step 6: Implement the read-only exact-source projection**

Add and register `get_shot_image_to_video_source`. It scopes Shot and version through the project, reads only `keyframe_asset_version_id`, and returns:

```ts
export interface ShotImageToVideoSource {
  assetId: string;
  assetVersionId: string;
  versionNumber: number;
  filePath: string;
  thumbnailPath: string | null;
  mimeType: string;
}
```

The command returns `SourceKeyframeMissing`, `AssetVersionNotFound`, or `SourceMediaTypeUnsupported` for expected invalid states. It is display-only; run creation independently revalidates and freezes the authoritative source.

- [ ] **Step 7: Add failing Shot panel tests**

```tsx
it("disables generation without an exact keyframe", async () => {
  render(<ShotImageToVideo projectRootPath="C:/project" sceneId="scene-1" shot={shot({ keyframeAssetVersionId: null })} onShotChanged={vi.fn()} />);
  expect(screen.getByRole("button", { name: "Generate Video" })).toBeDisabled();
  expect(screen.getByText("Add or generate a keyframe first.")).toBeInTheDocument();
});

it("creates the exact Shot I2V payload once on rapid clicks", async () => {
  render(<ShotImageToVideo projectRootPath="C:/project" sceneId="scene-1" shot={shotWithKeyframe()} onShotChanged={vi.fn()} />);
  await user.type(screen.getByLabelText("Motion prompt"), "Slow push-in");
  const button = screen.getByRole("button", { name: "Generate Video" });
  await Promise.all([user.click(button), user.click(button)]);
  expect(createWorkflowRun).toHaveBeenCalledTimes(1);
  expect(createWorkflowRun).toHaveBeenCalledWith("C:/project", "scene-builder", "1.0.0", "shot.image_to_video", {
    sceneId: "scene-1",
    shotId: "shot-1",
    providerId: "i2v",
    modelId: "motion-v1",
    prompt: "Slow push-in",
    generationParameters: { durationSeconds: 4 },
  });
});
```

Add a completed-run test rendering a video candidate and `Use for Shot`, then assert the promotion command receives current pin as expected value.

- [ ] **Step 8: Run Shot panel tests and verify RED**

```powershell
pnpm --filter @cinematic/desktop test -- ShotImageToVideo.test.tsx
```

Expected: component does not exist.

- [ ] **Step 9: Implement the Shot-local panel**

```ts
interface ShotImageToVideoProps {
  projectRootPath: string;
  sceneId: string;
  shot: Shot;
  onShotChanged(): void;
}

export function ShotImageToVideo(props: ShotImageToVideoProps): JSX.Element;
```

State covers provider/model, prompt, duration, optional aspect/FPS/seed, synchronous `creatingRef`, run detail, results, promotion state, and last valid error/status. Load `getShotImageToVideoSource` for the exact keyframe preview and secondary version id. Render the I2V selector, controls, plain-language disabled reason, `WorkflowRunView`, `GenerationResultCard`, `Use for Shot`, and current pin marker.

- [ ] **Step 10: Integrate into SceneShots and run source/UI tests GREEN**

Render behind a `Generate video` expander so only the selected Shot panel is active. Refresh Shots after promotion.

```powershell
pnpm --filter @cinematic/desktop test -- ProviderModelFields.test.tsx ShotImageToVideo.test.tsx SceneShots.goldenpath.test.tsx
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test cinema_commands_crud shot_i2v_source_returns_the_exact_pinned_version_not_latest -- --nocapture
```

- [ ] **Step 11: Commit the creator flow**

```powershell
git add apps/desktop/src/features/providers apps/desktop/src/features/scenes apps/desktop/src/features/workflows/api.ts apps/desktop/src-tauri/src/cinema apps/desktop/src-tauri/src/lib.rs apps/desktop/src-tauri/tests/cinema_commands_crud.rs packages/domain/src/cinema.ts
git commit -m "ui: add shot image-to-video generation flow"
```

---

### Task 6: Restore Durable Shot Status and Preserve Last Valid UI State

**Files:**
- Modify: `apps/desktop/src/features/scenes/ShotImageToVideo.tsx`
- Modify: `apps/desktop/src/features/scenes/ShotImageToVideo.test.tsx`
- Verify: `apps/desktop/src/features/workflows/WorkflowRunView.tsx`
- Verify: `apps/desktop/src/features/workflows/WorkflowRunView.background.test.tsx`

**Interfaces:**
- Consumes: persisted run list/detail and centralized `WorkflowRunView` observation.
- Produces: remount restoration without a second timer and transient-read resilience.

- [ ] **Step 1: Add failing restoration and cleanup tests**

```tsx
it("restores the latest persisted run for this Shot after remount", async () => {
  vi.mocked(listWorkflowRuns).mockResolvedValue([
    runRecord({ id: "other", inputJson: JSON.stringify({ shotId: "shot-2" }) }),
    runRecord({ id: "mine", operationId: "shot.image_to_video", inputJson: JSON.stringify({ shotId: "shot-1" }) }),
  ]);
  vi.mocked(getWorkflowRun).mockResolvedValue(runningDetail("mine"));
  render(<ShotImageToVideo {...props} />);
  expect(await screen.findByText(/Generating/)).toBeInTheDocument();
  expect(getWorkflowRun).toHaveBeenCalledWith("C:/project", "mine");
});

it("keeps the last valid run across a transient read failure", async () => {
  vi.mocked(getWorkflowRun).mockResolvedValueOnce(runningDetail("mine")).mockRejectedValueOnce(new Error("temporary"));
  render(<ShotImageToVideo {...props} />);
  expect(await screen.findByText(/Generating/)).toBeInTheDocument();
  expect(screen.queryByText("temporary")).not.toBeInTheDocument();
});
```

Retain the existing `WorkflowRunView.background.test.tsx` unmount/terminal timer assertions.

- [ ] **Step 2: Run restoration tests and verify RED**

```powershell
pnpm --filter @cinematic/desktop test -- ShotImageToVideo.test.tsx WorkflowRunView.background.test.tsx
```

- [ ] **Step 3: Implement restoration through WorkflowRunView only**

```ts
const records = await listWorkflowRuns(projectRootPath);
const latest = records
  .filter((record) => record.operationId === "shot.image_to_video")
  .filter((record) => parseInput(record.inputJson).shotId === shot.id)
  .sort((a, b) => b.createdAt.localeCompare(a.createdAt))[0];
if (latest) setRun(await getWorkflowRun(projectRootPath, latest.id));
```

Do not add `setInterval` to `ShotImageToVideo`. Pass restored detail to `WorkflowRunView`, retain prior detail on read failure, load results after completion, and guard async effects with `cancelled`.

- [ ] **Step 4: Run restoration regressions and verify GREEN**

```powershell
pnpm --filter @cinematic/desktop test -- ShotImageToVideo.test.tsx WorkflowRunView.background.test.tsx JobsPanel.test.tsx
```

- [ ] **Step 5: Commit status restoration**

```powershell
git add apps/desktop/src/features/scenes/ShotImageToVideo.tsx apps/desktop/src/features/scenes/ShotImageToVideo.test.tsx
git commit -m "ui: restore durable shot video generation status"
```

---

### Task 7: Add the Complete Golden Path, Replay, and Race Coverage

**Files:**
- Create: `apps/desktop/src-tauri/tests/shot_image_to_video_golden_path.rs`
- Modify: `apps/desktop/src-tauri/tests/background_video_job_acceptance.rs`
- Modify: `apps/desktop/src/features/scenes/SceneShots.goldenpath.test.tsx`
- Modify: `apps/desktop/src/__tests__/mvp-golden-path.test.tsx` only if command facade registration requires it

**Interfaces:**
- Consumes: Tasks 1-6.
- Produces: end-to-end evidence for source immutability, one submit, restart, lineage, replay, promotion, and unchanged keyframe.

- [ ] **Step 1: Write failing Rust golden path**

```rust
#[test]
fn shot_image_to_video_golden_path_survives_restart_and_promotes_exact_output() {
    let fixture = support::compilable_scene();
    let shot = &fixture.shots[0];
    let source_version = pin_exact_keyframe(&fixture.root, &fixture.scene.id, &shot.id);
    let server = install_loopback_i2v_provider(&fixture.root);
    let created = create_shot_i2v_run(&fixture.root, &fixture.scene.id, &shot.id);
    let waiting = WorkflowRuntime::advance_run(&fixture.root, &created.run.id).unwrap();
    approve_and_advance(&fixture.root, &waiting.run.id);
    assert_eq!(server.submit_count(), 1);
    assert_eq!(provider_job_operation(&fixture.root, &waiting.run.id), "video.imageToVideo");
    let replacement = newer_keyframe(&fixture);
    CinemaService::set_shot_keyframe(&fixture.root, &shot.id, Some(&replacement.id)).unwrap();
    background::reset_provider_cache_for_tests();
    drive_background_to_completion(&fixture.root, &waiting.run.id);
    assert_eq!(server.submit_count(), 1);
    let artifact = only_artifact(&fixture.root, &waiting.run.id);
    let lineage = GenerationService::get_generated_artifact(&fixture.root, &artifact.id).unwrap().lineage.unwrap();
    assert_eq!(lineage.source_asset_version_ids, vec![source_version.id.clone()]);
    assert!(CinemaService::list_shots(&fixture.root, &fixture.scene.id).unwrap()[0].generated_video_asset_version_id.is_none());
    let promoted = promote_shot_video_candidate(&fixture.root, &shot.id, &artifact.id, None).unwrap();
    let final_shot = &CinemaService::list_shots(&fixture.root, &fixture.scene.id).unwrap()[0];
    assert_eq!(final_shot.generated_video_asset_version_id.as_deref(), Some(promoted.asset_version_id.as_str()));
    assert_eq!(final_shot.keyframe_asset_version_id.as_deref(), Some(replacement.id.as_str()));
    assert_eq!(compiled_source_version(&fixture.root, &waiting.run.id), source_version.id);
}
```

- [ ] **Step 2: Add replay and race tests**

Add deterministic tests: completion twice yields one result/artifact/version/lineage; late cancellation cannot replace success; two null-expected promotions yield one winner and one `PromotionConflict`; duplicate normalized creation yields one run/submit; changed prompt allows a new run.

- [ ] **Step 3: Run acceptance test and verify RED**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test shot_image_to_video_golden_path -- --nocapture
```

- [ ] **Step 4: Fix only defects exposed by focused failing tests**

For each defect, first add a focused regression test in the owning module, then change only P10.2 seams: normalization, capability, attachment submission, operation persistence, completion dispatch, lineage, candidate dedupe, promotion CAS, or UI payload.

- [ ] **Step 5: Extend frontend golden path**

Drive keyframe-present -> generate -> running -> completed candidate -> `Use for Shot` with `StatefulTauriFacade`. Assert payload and exact pinned marker.

```powershell
pnpm --filter @cinematic/desktop test -- SceneShots.goldenpath.test.tsx
```

- [ ] **Step 6: Run focused P10.2 suites**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml shot_i2v -- --nocapture
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test background_video_job_acceptance shot_i2v -- --nocapture
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test shot_image_to_video_golden_path -- --nocapture
pnpm --filter @cinematic/desktop test -- ShotImageToVideo.test.tsx ProviderModelFields.test.tsx SceneShots.goldenpath.test.tsx WorkflowRunView.background.test.tsx JobsPanel.test.tsx
```

- [ ] **Step 7: Commit lifecycle coverage**

```powershell
git add apps/desktop/src-tauri/tests apps/desktop/src/features/scenes apps/desktop/src/__tests__
git commit -m "test: cover shot image-to-video lifecycle"
```

---

### Task 8: Full Regression Gates and Final Invariant Review

**Files:**
- Modify only files required by a newly failing regression, with a failing focused test first.
- Review every file changed since `d55179d` and every commit after `d7ecdf3`.

**Interfaces:**
- Consumes: completed P10.2 implementation.
- Produces: verified automated gates, clean diff/worktree, and final-report evidence.

- [ ] **Step 1: Format Rust**

```powershell
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml
```

- [ ] **Step 2: Run full Rust suite**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
```

Expected: all unit/integration tests pass; record counts and duration.

- [ ] **Step 3: Run all frontend/domain tests**

```powershell
pnpm -r test
```

- [ ] **Step 4: Run typecheck and production frontend build**

```powershell
pnpm --filter @cinematic/desktop exec tsc --noEmit
pnpm --filter @cinematic/desktop exec vite build
```

- [ ] **Step 5: Run Clippy and compare warnings**

```powershell
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
```

Expected: zero warnings. If baseline requires warnings, rerun without `-D warnings`, compare exactly, and fix every new warning.

- [ ] **Step 6: Run format and whitespace checks**

```powershell
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --check
git diff --check
```

- [ ] **Step 7: Build Tauri production bundle**

```powershell
pnpm --filter @cinematic/desktop tauri build
```

Expected: bundle succeeds; record artifact paths. This is not the manual clean-install gate.

- [ ] **Step 8: Perform final invariant searches**

```powershell
rg -n "loop \{|while .*poll|sleep\(" apps/desktop/src-tauri/src apps/desktop/src
rg -n "canonical_version_id|ORDER BY .*version|latest" apps/desktop/src-tauri/src/workflow apps/desktop/src-tauri/src/cinema
rg -n "provider.*==|model.*contains|includes\(.*model" apps/desktop/src-tauri/src apps/desktop/src
rg -n "set_shot_video|generated_video_asset_version_id" apps/desktop/src-tauri/src/workflow apps/desktop/src-tauri/src/providers
rg -n "video.imageToVideo|supports_image_to_video|supportsImageToVideo" apps/desktop/src-tauri/src apps/desktop/src packages/domain/src
```

Inspect every hit for blocking polling, latest-version resolution, name routing, auto-promotion, and missing capability enforcement.

- [ ] **Step 9: Inspect migration, diff, commits, and status**

```powershell
git diff d55179d --stat
git diff d55179d -- apps/desktop/src-tauri/src/db/migrations.rs
git diff d55179d
git log --oneline d55179d..HEAD
git status --short --branch
```

Expected: no unapproved migration, no unrelated edits, coherent commits, clean worktree.

- [ ] **Step 10: Commit a verification correction only when required**

```powershell
git add -u
git commit -m "fix: harden shot image-to-video lifecycle"
```

Do not create an empty commit.

- [ ] **Step 11: Prepare final report sections A-Y**

Distinguish `IMPLEMENTATION COMPLETE` from `RELEASE READY`; mark Manual GUI and Clean Install open unless performed; list commands/results, final HEAD, commits, risks, and recommended next step.

---

## Plan Self-Review Checklist

- Every non-negotiable invariant maps to Tasks 1-8.
- Exact source binding occurs at run creation before Shot drift.
- Request additions are backward compatible.
- Provider/model checks use the declarative operation truth.
- Submission includes verified attachments and persists `video.imageToVideo`.
- Retry uses the compiled request and never rereads the Shot.
- Completion captures a candidate and never pins the Shot.
- Promotion validates immutable lineage, replays safely, and compare-and-set protects races.
- UI restores through `WorkflowRunView`, not a second timer.
- Cold-adapter restart, one-submit, replay, cancellation, double-click, and promotion races have deterministic tests.
- No schema migration is planned.
- P10.1, scene video, keyframe, generation, JobsPanel, and WorkflowRunView regressions are explicitly run.
