# P10.2 Shot-Scoped Image-to-Video Design

**Date:** 2026-08-31

**Baseline:** `master@d55179d`

**Status:** Approved for implementation planning

## Purpose

P10.2 lets a creator generate a video from the exact keyframe pinned to a Shot, monitor the generation through Cinery's durable provider runtime, review the generated candidate, and explicitly promote one exact output version back to that Shot.

The implementation extends the current workflow, provider, generation, lineage, asset-version, and Cinema systems. It does not add a second job runner, a provider-specific Shot path, automatic promotion, or mutable/latest-version resolution.

## Existing Foundation

The repository already provides the required durable and normalized foundation:

- `scene_shots.keyframe_asset_version_id` and `generated_video_asset_version_id` are exact `AssetVersion` pins.
- workflow run input, context snapshots, and compiled request step output are immutable persisted JSON.
- `ExecutionRequest.references` and ephemeral verified provider attachments carry exact media inputs.
- provider capabilities already include `supports_image_to_video`.
- declarative providers already recognize `video.imageToVideo`, map reference attachments, persist provider operation identity, and support cold rehydration.
- provider attempts, `provider_jobs`, the `BackgroundJobRunner`, retries, cancellation, completion ordering, and restart recovery are established by P10.1.
- generation result sets, generated artifacts, artifact sources, lineage, promotions, candidate `AssetVersion`s, and audit events already capture the result lifecycle.
- `WorkflowRunView`, `JobsPanel`, `GenerationResults`, and `ProviderModelFields` provide the reusable UI surfaces.

P10.2 therefore requires no parallel domain aggregate and no new polling runtime.

## Alternatives Considered

### Selected: Dedicated Shot I2V Operation

Add `shot.image_to_video` to the existing `scene-builder` skill. Give it a dedicated context resolver and compiler, then route execution through the existing video execution and completion machinery.

This keeps the product operation explicit, gives capability and error handling unambiguous semantics, preserves `scene.generate_video`, and makes retries and provenance easy to reason about.

### Rejected: Overload `scene.generate_video`

An optional `shotId` could make `scene.generate_video` switch between text-to-video and image-to-video. The diff would be smaller, but one operation identifier would describe two materially different requests, validation rules, capabilities, source lineages, and UI flows. That ambiguity would make provider routing and future maintenance less safe.

### Rejected: New Shot Generation Tables or Runtime

A separate `ImageToVideoGeneration` table or polling service would duplicate immutable workflow input, provider attempts/jobs, result sets, lineage, and Shot pins. It would also violate the requirement to build on P10.1.

## Operation Contract

The new operation is registered under the existing `scene-builder` skill:

```text
operation id: shot.image_to_video
input schema: generate_shot_image_to_video
resolver: shot_image_to_video_context
compiler: shot_image_to_video_v1
expected output: video candidate owned by sceneId
```

The command accepts this semantic input:

```ts
interface ShotImageToVideoInput {
  sceneId: string;
  shotId: string;
  providerId: string;
  modelId: string;
  prompt: string;
  generationParameters?: {
    aspectRatio?: string;
    durationSeconds?: number;
    fps?: number;
    seed?: number;
  };
}
```

During run creation, the runtime opens an immediate transaction, validates the Shot, reads its exact keyframe pin once, and enriches the persisted input with `sourceAssetVersionId`. If `durationSeconds` is omitted, the same transaction copies the Shot duration into the persisted generation parameters. This closes the gap between run creation and the later context step: once the run exists, the source version and duration are already immutable.

The UI only exposes controls represented by the provider-neutral request model and disables controls known to be unsupported by the selected provider. Provider/model selection remains execution metadata, while prompt, source version, and generation parameters are persisted before execution and compiled into the immutable request.

## Exact Shot and Keyframe Resolution

Run creation and context resolution divide validation without ever resolving mutable state twice:

1. Under the run-creation immediate transaction, load the Shot by `shotId` within the requested `sceneId` and project.
2. Reject a missing or deleted Shot with `ShotNotFound`.
3. Read `scene_shots.keyframe_asset_version_id` exactly once and persist it as `sourceAssetVersionId` in the immutable run input.
4. Reject a missing pin with `SourceKeyframeMissing`.
5. During context resolution, load that persisted exact `AssetVersion` within the same project; never reread the Shot pin, follow `assets.canonical_version_id`, or choose the latest version.
6. Require asset type `shot_keyframe` or `image` and MIME type `image/*`; otherwise return `SourceMediaTypeUnsupported`.
7. Require the stored file to exist. Hash and size verification remains in `resolve_reference_attachments` immediately before submission.
8. Persist the exact version in `WorkflowContextSnapshot.assets` and the immutable run identifiers in `resolved_context`.

The asset snapshot status model gains a `pinned` value. This is deliberate: a Shot's exact historical keyframe remains a valid immutable input even if a newer version later becomes canonical. Existing canonical snapshots continue to serialize unchanged.

After run creation succeeds, Shot or Canon mutations cannot alter the chosen source version. After the context step completes, no mutable state can alter the snapshot, compiled request, retry input, or provider submission.

## Execution Request

Add these provider-neutral execution concepts:

```text
ExecutionTask::ShotImageToVideo
ReferenceRole::SourceImage
ExecutionRequest.generation_parameters
```

`generation_parameters` is additive and defaults to empty when deserializing older persisted requests. The provider request maps it into the existing `ProviderGenerationParameters` structure.

The compiler produces the semantic equivalent of:

```json
{
  "requestVersion": 1,
  "task": "shot_image_to_video",
  "mediaType": "video",
  "prompt": "<creator prompt plus stable shot motion context>",
  "references": [
    {
      "type": "asset_version",
      "reference": "<exact keyframe AssetVersion id>",
      "description": "Source keyframe for Shot <id>",
      "role": "source_image"
    }
  ],
  "generationParameters": {
    "durationSeconds": 4
  },
  "constraints": [],
  "expectedOutput": {
    "assetType": "video",
    "mediaType": "video",
    "desiredStatus": "candidate",
    "ownerEntityInputRef": "sceneId"
  },
  "provenance": {
    "workflowRunId": "<run>",
    "skillId": "scene-builder",
    "skillVersion": "1.0.0",
    "operationId": "shot.image_to_video"
  }
}
```

The prompt compiler uses the persisted creator prompt and generation parameters. The UI may seed the prompt from visible Shot intent/action/camera, but the compiler does not reread those mutable fields and does not require a scene compilation.

## Provider Capability and Model Routing

Provider routing is capability-based at every layer:

- `ProviderCapabilities::supports` requires `supports_image_to_video` for `ShotImageToVideo` or any video request carrying a source-image reference.
- declarative adapters select only `video.imageToVideo` for this request.
- the current fallback from missing `video.imageToVideo` to `video.generate` is removed.
- declarative model capability lists are checked against `video.imageToVideo`; model-name matching is forbidden.
- the provider/model UI requests the same operation capability and displays only compatible options.
- configuration and credential checks continue through `ProviderService`.

Adapters receive verified attachment bytes and metadata. They never query Shot, Canon, asset, or workflow state.

## Durable Submission and Restart

The execution path reuses the P10.1 order:

```text
create attempt
-> resolve and verify exact reference attachment
-> submit provider request
-> persist ProviderJob including operation
-> return to UI
```

The background runner then discovers, claims, polls, records progress, fetches, captures, and completes the attempt/run. No Tauri command blocks in a remote polling loop.

The persisted provider job operation must be `video.imageToVideo` for declarative I2V jobs. Rehydration constructs a fresh adapter from the persisted provider definition and polls/fetches the same remote operation and provider job id. Reopening the project attaches the existing runner and never submits again.

## Retry and Cancellation

Retry uses the existing failed run and compiled request step output:

- the exact source `AssetVersion` remains unchanged;
- prompt and generation parameters remain unchanged;
- a new immutable attempt and idempotency key are created;
- the Shot is not re-read;
- the provider is submitted once for the new attempt.

Cancellation continues through `cancellation_requested` and the background runner. UI copy reflects `supports_cancel`; a local terminal cancellation is never described as a confirmed remote cancellation when the provider cannot cancel.

## Completion, Candidate Capture, and Idempotency

On provider success, the shared completion module:

1. fetches the video through the provider adapter;
2. captures one video result set/artifact for the attempt;
3. records the exact keyframe version as the artifact source and lineage source;
4. imports the artifact into the existing stable scene-owned video asset as a candidate version;
5. leaves the Shot's video pin untouched;
6. marks attempt, job, step, and run complete only after durable capture.

Completion recognizes `shot.image_to_video` as a one-output video workflow and reuses the scene video candidate importer. Result-set uniqueness by provider attempt, content hashes, duplicate-version reconciliation, and terminal guards preserve replay idempotency.

A crash/replay cannot create a second result set, artifact, asset version, lineage row, or provider submission.

## Provenance

The existing immutable records jointly answer the required provenance questions:

| Question | Durable source |
| --- | --- |
| Initiating Shot | workflow input `shotId` and promotion audit event |
| Exact source keyframe | context snapshot, request reference, artifact source, lineage source ids |
| Immutable request | compile-step output and compiled request SHA-256 |
| Provider/model | provider attempt and artifact lineage |
| Attempt | generation result set and artifact lineage |
| Provider job | `provider_jobs` joined through the attempt |
| Result/result set | generation result set and generated artifact |
| Output AssetVersion | candidate import, artifact promotion, and Shot promotion audit event |

No provenance is reconstructed from the Shot's current mutable pins.

## Explicit Shot Promotion

Add one application command with the semantic input:

```ts
promoteShotVideoCandidate({
  projectRootPath,
  shotId,
  artifactId,
  expectedCurrentVideoAssetVersionId
})
```

The command:

1. validates the artifact is available video output from `shot.image_to_video`;
2. validates the immutable run input names the same Shot and Scene;
3. validates lineage contains the exact frozen source keyframe;
4. resolves the already-imported candidate `AssetVersion` by artifact hash in the scene-owned video asset;
5. idempotently promotes that exact version using current asset history semantics;
6. compare-and-set updates the Shot's exact video pin against `expectedCurrentVideoAssetVersionId`;
7. appends `shot.video.promoted` audit metadata containing Shot, source keyframe, run, attempt, artifact, prior pin, and promoted version.

The command is crash-reconcilable: artifact/version promotion is idempotent, and replay completes the Shot pin if an interruption occurs between phases. Repeating the same promotion returns the same exact version. A stale expected pin or conflicting candidate returns typed `PromotionConflict` without overwriting the winner.

The source keyframe pin and prior candidates are never modified or deleted.

## Double-Click and Race Semantics

### Generate Double-Click

The Shot UI uses a synchronous ref guard in addition to disabled state. The run-creation immediate transaction captures `sourceAssetVersionId`, then finds and returns an existing active logical run for the same Shot, exact source version, provider/model, prompt, and generation parameters. A changed input or a terminal prior run represents a new generation.

### Cancel vs Completion

The existing terminal compare-and-set behavior remains authoritative. A late cancel cannot replace successful completion or remove captured output.

### Promotion Race

Promotion uses an expected-current-version compare-and-set. Two conflicting candidates cannot both silently win. The loser receives `PromotionConflict`, and replaying the winner is idempotent.

## Error Contract

Expected product states use typed `AppError` variants and stable command error codes:

- `ShotNotFound`
- `SourceKeyframeMissing`
- `AssetVersionNotFound`
- `SourceMediaTypeUnsupported`
- `ImageToVideoUnsupported`
- existing provider configuration and submission errors
- existing remote generation, fetch, and capture errors
- `PromotionConflict`
- existing generation lineage/promotability errors

SQLite and internal transport errors are translated before reaching normal UI states.

## Shot Workspace UX

The workflow lives in `SceneShots`, not a standalone developer screen. A focused Shot generation panel or extracted Shot-local component provides:

- the selected Shot number and intent;
- source keyframe thumbnail;
- concise exact-version identity in secondary metadata;
- I2V-capable configured provider and model selectors;
- prompt seeded from frozen Shot motion context but editable by the creator;
- duration, aspect ratio, FPS, and seed controls when supported;
- a primary `Generate Video` action with plain-language disabled reasons;
- `WorkflowRunView` status and cancellation/retry controls;
- candidate playback through existing media preview infrastructure;
- provider/model and attempt metadata in review details;
- explicit `Use for Shot` promotion;
- a clear marker for the currently pinned Shot video version.

The implementation follows the existing visual system, spacing, typography, controls, and accessibility conventions. It does not introduce a new aesthetic layer or unrelated Cinema rewrite.

## Status Restoration

When the Shot workspace mounts, it loads persisted workflow runs, finds the latest `shot.image_to_video` run whose immutable input matches the Shot, and loads its detail. Active runs render through `WorkflowRunView`; completed runs load persisted generation results.

No new polling loop is introduced. `WorkflowRunView` remains the centralized observer, stops at terminal state, avoids overlap, cleans up on unmount, and preserves the last valid state across transient read failures.

## Schema and Migration Review

No migration is planned. Existing tables already store every normalized relationship required for execution, restart, result capture, lineage, promotion, and exact Shot pins.

Additive Rust/TypeScript enum and JSON fields are sufficient. Old compiled requests remain readable through defaulted generation parameters. If implementation proves that an invariant cannot be represented by the existing immutable workflow input or audit metadata, work stops for design review before adding migration `0023`.

## Test Strategy

Implementation follows red-green-refactor in vertical slices.

### Domain and Compiler

- exact pinned keyframe compiles as the sole `source_image` reference;
- missing Shot/keyframe/version and non-image inputs return typed errors;
- superseded but still pinned exact versions remain valid;
- mutating the Shot/keyframe after compilation does not change the request;
- old requests deserialize with empty generation parameters.

### Provider

- provider and model without `video.imageToVideo` are rejected before submission;
- declarative I2V never falls back to `video.generate`;
- verified reference bytes reach multipart/template mapping;
- persisted operation is `video.imageToVideo`.

### Runtime and Restart

- submission returns with a durable ProviderJob;
- double advance and double create do not duplicate a logical execution;
- background ticks complete deterministically without sleeps;
- cold adapter recreation polls and fetches the same job;
- restart produces exactly one remote submit;
- retry preserves the request/source and creates a fresh attempt/key;
- cancellation retains P10.1 truthful semantics.

### Completion and Promotion

- candidate video and exact-source lineage are captured;
- replay creates no duplicate result set, artifact, version, or lineage;
- completion never pins the Shot;
- explicit promotion pins the exact output and leaves keyframe unchanged;
- same promotion is idempotent;
- conflicting promotion returns `PromotionConflict`;
- historical versions remain accessible.

### UI

- no-keyframe and no-capable-provider states explain why generation is unavailable;
- provider/models are capability-filtered;
- payload includes Shot, prompt, and parameters;
- persisted active status resumes after remount;
- terminal state stops observation;
- transient read failure preserves last known state;
- candidate video appears after completion;
- `Use for Shot` updates the exact pin;
- cleanup prevents timers/effects after unmount;
- synchronous click guard prevents duplicate creates.

### Golden Path

The acceptance test covers project/Scene/Shot creation, exact keyframe pinning, I2V-capable loopback provider selection, immutable request compilation, durable submission, simulated restart and cold adapter recreation, background completion, candidate capture, exact-source lineage, explicit promotion, unchanged keyframe pin, exact output pin, and exactly one remote submission.

## Implementation Slices

1. Add exact Shot/keyframe context resolution and provider-neutral request compilation.
2. Enforce provider/model I2V capability and adapter operation selection.
3. Route submission and completion through the P10.1 durable job runner.
4. Capture candidate video and exact-source lineage idempotently.
5. Add crash-reconcilable, conflict-safe Shot promotion.
6. Add Shot workspace generation, restoration, review, and promotion UX.
7. Add restart, retry, cancellation, replay, and race acceptance coverage.
8. Run all repository verification gates and complete the final invariant review.

## Verification Gates

Before completion, run the repository equivalents of:

```text
cargo test
pnpm -r test
pnpm exec tsc --noEmit
pnpm vite build
cargo clippy --all-targets
cargo fmt --check
git diff --check
```

Run the Tauri production bundle if the repository's current release gate includes it. Compare Clippy warnings with the baseline. Manual GUI and clean-install checks remain explicitly open unless actually executed.

## Definition of Done

P10.2 implementation is complete only when exact keyframe binding, immutable compilation, capability-based I2V routing, durable restart-safe execution, idempotent candidate capture, exact lineage, explicit conflict-safe Shot promotion, complete Shot UX, deterministic coverage, and automated build gates all pass.

`IMPLEMENTATION COMPLETE` does not mean `RELEASE READY` while manual GUI or clean-install release gates remain unperformed.
