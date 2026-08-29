# Character Result Gallery and Promotion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Turn Production into a persistent Face → Outfit → Character Sheet workflow whose completed candidates can be reviewed, saved, and deliberately promoted to the correct character-owned asset.

**Architecture:** Derive a provider-neutral `GenerationResultContext` from persisted workflow run detail and the selected operation’s expected output. A reusable results surface loads all persisted result sets, resolves only eligible target assets, optionally creates a correctly typed/owned asset, and invokes the existing backend promotion command. Production owns the guided flow; Workflows remains the technical/history view but renders the same results component.

**Tech Stack:** React, TypeScript, Vitest, React Testing Library; existing Tauri IPC commands, Rust workflow/generation services, SQLite repositories.

**Spec:** `docs/superpowers/specs/2026-08-29-character-results-design.md`

## Global Constraints

- Promotion is always explicit; never auto-promote a candidate.
- The backend must validate project, expected asset type, and owner entity even if the UI already filtered targets.
- Completed result galleries survive navigation and application restart.
- Face source remains optional; Outfit and Character Sheet must make prerequisites visible before launch.
- Production is the guided surface; Workflows retains diagnostics, history, retry, and recovery.
- Reuse the current dark neutral/red design system, semantic tokens, focus styles, and calm motion.
- Preserve the dirty working tree and stage only agent-owned hunks.

---

### Task 1: Define and derive a generalized generation result context

**Files:**
- Modify: `packages/domain/src/generation.ts`
- Modify: `packages/domain/src/workflow.ts`
- Modify: `packages/domain/src/skill.ts`
- Modify: `packages/domain/src/index.ts`
- Test: `packages/domain/src/generation.test.ts`
- Modify: `apps/desktop/src/features/workflows/api.ts`

**Interface produced:**

```ts
export interface GenerationResultContext {
  workflowRunId: string;
  operationId: string;
  expectedAssetType: AssetType;
  ownerEntityId: string | null;
  resultSets: GenerationResultSetDetail[];
}

export function deriveGenerationResultContext(
  run: WorkflowRunDetail,
  operation: SkillOperation,
): GenerationResultContext | null;
```

- [x] **Step 1: Write failing domain tests**

Cover Face Lock, Outfit, Character Sheet, non-generative operations, missing owner metadata, multiple result sets, and stable serialization after reload.

- [x] **Step 2: Run RED**

Run: `pnpm --filter @cinematic/domain test -- generation.test.ts`

- [x] **Step 3: Implement the smallest pure derivation**

Use `operation.expectedOutput.assetType`; obtain `ownerEntityId` from persisted run input/context rather than component state. Return `null` when there is no promotable generated output.

- [x] **Step 4: Run GREEN and type-check consumers**

Run: `pnpm --filter @cinematic/domain test`

Suggested commit: `feat: define generation result context`

### Task 2: Enforce promotion eligibility at the command boundary

**Files:**
- Modify: `apps/desktop/src-tauri/src/generation/service.rs`
- Modify: `apps/desktop/src-tauri/src/generation/commands.rs`
- Modify: `apps/desktop/src-tauri/src/generation/repository.rs`
- Modify: `apps/desktop/src-tauri/src/workflow/artifacts.rs`
- Test: `apps/desktop/src-tauri/tests/generation_promotion.rs`
- Test: `apps/desktop/src-tauri/tests/character_pipeline_acceptance.rs`

**Interface consumed:**

```rust
pub struct PromoteGeneratedArtifactRequest {
    pub project_id: String,
    pub workflow_run_id: String,
    pub result_set_id: String,
    pub artifact_id: String,
    pub target_asset_id: String,
    pub make_canonical: bool,
}
```

- [x] **Step 1: Add failing negative tests**

Assert rejection for a target in another project, wrong asset type, wrong owner, artifact outside the run/result set, duplicate promotion, and an invalid canonical request. Assert Outfit and Sheet succeed for matching character-owned targets.

- [x] **Step 2: Run RED**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml generation_promotion character_pipeline_acceptance -- --nocapture`

- [x] **Step 3: Centralize eligibility validation**

```rust
fn validate_promotion_target(
    expected_type: AssetType,
    expected_owner: Option<&str>,
    target: &AssetRecord,
) -> Result<(), AppError> {
    ensure!(target.asset_type == expected_type, AppError::validation("asset_type"));
    ensure!(target.owner_entity_id.as_deref() == expected_owner, AppError::validation("owner"));
    Ok(())
}
```

Reuse it for every operation; do not special-case Face Lock in the command.

- [x] **Step 4: Run GREEN and commit**

Suggested commit: `fix: validate character result promotion targets`

### Task 3: Generalize the result gallery and promotion dialog

**Files:**
- Modify: `apps/desktop/src/features/production/GenerationResults.tsx`
- Modify: `apps/desktop/src/features/production/GenerationResultCard.tsx`
- Modify: `apps/desktop/src/features/production/PromoteArtifactDialog.tsx`
- Modify: `apps/desktop/src/features/production/api.ts`
- Create: `apps/desktop/src/features/production/GenerationResults.test.tsx`
- Modify: `apps/desktop/src/styles/app.css`

**Component contract:**

```ts
interface GenerationResultsProps {
  projectId: string;
  context: GenerationResultContext;
  onPromoted?(targetAssetId: string, versionId: string): void;
}
```

- [x] **Step 1: Write failing gallery tests**

Render persisted Face, Outfit, and Sheet result sets. Verify candidate image, metadata, QA state, save action, optional canonical checkbox, loading/empty/error states, and keyboard-accessible dialog behavior.

- [x] **Step 2: Write failing target-filter tests**

Mock mixed project/type/owner assets. Assert only eligible assets appear. If none exist, show “Create asset” rather than a dead-end select.

- [x] **Step 3: Run RED**

Run: `pnpm --filter @cinematic/desktop test -- GenerationResults.test.tsx`

- [x] **Step 4: Implement a type-neutral gallery**

Remove hard-coded Face Lock labels and target type. Use `context.expectedAssetType` and `ownerEntityId` for copy and filters. Keep result cards as a gallery/list hybrid, not nested decorative cards.

- [x] **Step 5: Run GREEN and accessibility checks**

Run: `pnpm --filter @cinematic/desktop test -- GenerationResults.test.tsx`

Suggested commit: `feat: generalize character result gallery`

### Task 4: Add inline target-asset creation without auto-promotion

**Files:**
- Modify: `apps/desktop/src/features/production/PromoteArtifactDialog.tsx`
- Modify: `apps/desktop/src/features/production/api.ts`
- Modify: `apps/desktop/src/features/assets/api.ts`
- Test: `apps/desktop/src/features/production/GenerationResults.test.tsx`
- Test: `apps/desktop/src/features/assets/api.test.ts`

**Flow produced:**

```ts
const target = await createAsset({
  projectId,
  assetType: context.expectedAssetType,
  ownerEntityId: context.ownerEntityId,
  name,
});
setSelectedTargetId(target.id); // still require explicit Promote click
```

- [x] **Step 1: Add failing tests for inline creation**

Assert the created asset receives exact type/owner, becomes selected, does not trigger promotion automatically, reports validation failures inline, and cannot be submitted twice while pending.

- [x] **Step 2: Run RED**

Run: `pnpm --filter @cinematic/desktop test -- GenerationResults.test.tsx api.test.ts`

- [x] **Step 3: Implement creation inside the dialog**

Use an inline disclosure with label and name field. Keep focus inside the dialog and return it to the triggering card on close.

- [x] **Step 4: Run GREEN and commit**

Suggested commit: `feat: create promotion targets from result gallery`

### Task 5: Build the guided Production Face → Outfit → Sheet flow

**Files:**
- Modify: `apps/desktop/src/features/production/ProductionWorkspace.tsx`
- Modify: `apps/desktop/src/features/production/CharacterBuilderOperation.tsx`
- Modify: `apps/desktop/src/features/production/AiDirectorBar.tsx`
- Modify: `apps/desktop/src/features/production/ProductionWorkspace.test.tsx`
- Modify: `apps/desktop/src/styles/app.css`

**State model:**

```ts
type CharacterStage = "face" | "outfit" | "sheet";

interface CharacterStageState {
  operationId: string;
  runId: string | null;
  status: "ready" | "blocked" | "running" | "completed" | "failed";
  blockers: ReadinessBlocker[];
}
```

- [x] **Step 1: Write failing guided-flow tests**

Assert Face source is optional; Outfit reads the promoted Face; Sheet reads promoted Face + Outfit; blockers include actionable links; stage selection and completed results persist while switching stages; provider/model selection is retained.

- [x] **Step 2: Run RED**

Run: `pnpm --filter @cinematic/desktop test -- ProductionWorkspace.test.tsx`

- [x] **Step 3: Implement semantic stage navigation and persistent result loading**

Use buttons/tabs with `aria-current`, a single main work area, and a compact prerequisite summary. Derive stage completion from backend run/asset state rather than a client-only wizard index.

- [x] **Step 4: Run GREEN and responsive checks**

Verify 1280px three-column shell fit, 768px stacked content, 200% zoom, focus order, and `prefers-reduced-motion`.

Suggested commit: `feat: guide character production stages`

### Task 6: Reuse completed result galleries in Workflow history

**Files:**
- Modify: `apps/desktop/src/features/workflows/WorkflowRunView.tsx`
- Modify: `apps/desktop/src/features/workflows/WorkflowWorkspace.tsx`
- Modify: `apps/desktop/src/features/workflows/WorkflowRunView.test.tsx`
- Modify: `apps/desktop/src/features/workflows/WorkflowWorkspace.test.tsx`

- [x] **Step 1: Write failing reload/history tests**

Load a completed Outfit or Sheet run directly by ID. Assert the shared gallery appears after navigation/reload and promotion updates the run detail without erasing technical metadata.

- [x] **Step 2: Run RED**

Run: `pnpm --filter @cinematic/desktop test -- WorkflowRunView.test.tsx WorkflowWorkspace.test.tsx`

- [x] **Step 3: Render the same `GenerationResults` from derived context**

Keep diagnostics, provider attempt, retry, and recovery controls in Workflows; do not duplicate promotion logic.

- [x] **Step 4: Run the slice gate**

Run: `pnpm test`

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test character_pipeline_acceptance -- --nocapture`

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test generation_promotion -- --nocapture`

Run: `pnpm --filter @cinematic/desktop build`

Run: `git diff --check`

- [x] **Step 5: Commit verified owned hunks**

Suggested commit: `feat: persist character results across production and workflows`
