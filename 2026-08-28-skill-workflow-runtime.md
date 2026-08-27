# Skill / Workflow Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for progress tracking.

**Goal:** Build the provider-independent Skill / Workflow Runtime for AI Cinematic Production OS. A skill must be a versioned executable production procedure rather than a prompt file. The runtime must load deterministic skill definitions, validate prerequisites against locked Canon and canonical Assets, snapshot all resolved context at workflow start, execute a resumable state machine with explicit approval gates, compile provider-neutral execution requests, persist every transition, survive application restart, and produce a dry-run output for one real cinematic workflow without calling any AI provider.

**Architecture:** Skills are declarative versioned definitions registered in application code and validated against a strict schema. A skill contains operations. An operation declares intent examples, prerequisites, input schema, workflow steps, expected output, and validators. `WorkflowRuntime` owns state transitions; individual skills do not perform arbitrary database writes. The runtime resolves current Canon/Asset authority once at launch, stores an immutable `context_snapshot`, and drives deterministic steps. Human approval is a first-class workflow state. Execution steps emit provider-neutral `ExecutionRequest` envelopes into a `DryRunExecutor` during P3. Real model/provider adapters are deferred to P4.

**Tech Stack:** Existing Tauri 2 + React + TypeScript + Vite workspace; project-local SQLite via Rust `rusqlite`; existing Canon Engine and Asset Manifest from P0/P2; TypeScript Zod for shared skill schemas; Rust `serde`/`serde_json`/`ulid`/`chrono`/`thiserror`; Vitest + React Testing Library; Rust unit/integration tests with `tempfile`.

**Prerequisite Plans:**
- `docs/superpowers/plans/2026-08-27-project-kernel-asset-versioning.md`
- `docs/superpowers/plans/2026-08-27-canon-engine.md`

**Master Spec:** `docs/specs/ai-cinematic-production-os-master-plan.md`

---

# 0. Entry Criteria

Do not begin P3 until P0/P1 and P2 acceptance are passing.

The implemented codebase is expected to expose equivalent production interfaces for:

```text
Project
- create/open project

Assets
- create conceptual asset
- import asset versions
- get asset + versions
- canonical version lookup
- explicit canonical promotion

Canon
- Story/Character/Location/etc. entities
- independently locked sections
- append-only canon revisions
- locked Character visual-lock query
- locked World Rule query
- locked Production Rule query
- open protected TBD query
```

Use the actual implemented signatures if they differ. Do not redesign working P0/P2 APIs to match this plan cosmetically. Add narrow adapter/query functions only where P3 needs them.

---

# 1. P3 Scope

Implement only **Skill / Workflow Runtime**.

## Included

- `SkillDefinition` schema
- versioned Skill Registry
- `SkillOperation` schema
- prerequisite declarations and evaluation
- protected-TBD guards
- structured workflow steps
- workflow-run persistence
- workflow-step persistence
- append-only workflow events
- immutable context snapshot
- input validation
- explicit approval gates
- rejection and cancellation
- deterministic resume after restart
- provider-neutral `ExecutionRequest`
- `DryRunExecutor`
- dry-run artifact persistence
- one production-grade sample skill: `character.create_face_lock`
- Skill/Workflow UI
- workflow event/history viewer
- end-to-end acceptance test

## Explicitly excluded

Do not implement:
- Gemini/OpenAI/Fal/Replicate APIs
- ComfyUI integration
- model downloads
- provider capability routing
- paid execution
- image/video generation
- VLM QA
- automatic repair
- Production Router / natural-language intent routing
- LLM-generated skill definitions
- arbitrary user-authored executable code
- marketplace
- third-party skill package installation
- cloud sync
- collaboration
- MCP

Provider work belongs to **P4 — Provider Adapter Layer**. QA belongs to a later Visual QA plan.

---

# 2. Product Context

The system separates four authorities:

```text
Narrative Canon
    ↓
Character Visual Canon
    ↓
Scene / World Visual Canon
    ↓
Temporal / Cinema Direction
```

P3 introduces the execution layer:

```text
USER ACTION
    ↓
SKILL OPERATION
    ↓
PREREQUISITE RESOLUTION
    ↓
CONTEXT SNAPSHOT
    ↓
WORKFLOW STATE MACHINE
    ↓
APPROVAL GATE
    ↓
PROVIDER-NEUTRAL EXECUTION REQUEST
    ↓
DRY RUN (P3)
```

A skill is not a prompt template. A skill is:

```text
Operation
↓
Prerequisites
↓
Inputs
↓
Workflow Steps
↓
State Transitions
↓
Request Compilation
↓
Expected Output Contract
```

---

# 3. Hard Architecture Rules

1. **Skill ≠ Model ≠ Provider.** P3 code must not reference specific generation providers.
2. **WorkflowRuntime owns state.** React components and skill compilers cannot directly mutate workflow state.
3. **Launch context is immutable.** Canon/Asset references and values are snapshotted at run time.
4. **Prerequisites use current authority only.** Locked Canon and canonical Asset versions count; draft/newest candidates do not.
5. **Approval is explicit.** Successful compilation never implies approval.
6. **Transitions are auditable.** Every state transition creates an append-only workflow event.
7. **Restart is deterministic.** Reopening the app never auto-advances a waiting/ready workflow.
8. **No arbitrary executable skill scripts in P3.** Skills are application-registered typed definitions.
9. **Protected TBDs block only when an operation declares a relevant guard.**
10. **External side effects only occur in explicit execute steps.**
11. **Historical runs preserve skill ID/version.**
12. **P3 must not create generated media Asset versions.** That begins when P4 returns provider media.

---

# 4. Reference Workflow

Implement one complete operation:

```text
character.create_face_lock
```

Flow:

```text
SELECT CHARACTER
      ↓
VALIDATE INPUT
      ↓
VALIDATE PREREQUISITES
      ↓
RESOLVE + SNAPSHOT CANON
      ↓
COMPILE FACE-LOCK REQUEST
      ↓
USER REVIEWS REQUEST
      ↓
APPROVE
      ↓
EXPLICIT EXECUTE DRY RUN
      ↓
COMPLETE
```

P3 records the expected output contract:

```text
assetType = face_lock
mediaType = image
desiredStatus = candidate
```

but does not create the media asset.

---

# 5. Skill Definition Model

Create shared domain types:

```ts
export interface SkillDefinition {
  id: string;
  name: string;
  version: string;
  description: string;
  operations: SkillOperation[];
}
```

P3 ships one builtin skill:

```text
character-builder@1.0.0
```

Use semantic version validation.

---

# 6. Skill Operation Model

```ts
export interface SkillOperation {
  id: string;
  name: string;
  description: string;
  intentExamples: string[];
  inputSchemaId: string;
  prerequisites: Prerequisite[];
  tbdGuards: TbdGuard[];
  workflow: WorkflowStepDefinition[];
  expectedOutput: ExpectedOutputDefinition | null;
}
```

Production operation:

```text
character.create_face_lock
```

---

# 7. Face Lock Input Schema

Detailed visual construction remains a Character Builder workflow input because the Story Bible owns only Bible-level visual identity.

```ts
export const createFaceLockInputSchema = z.object({
  projectRootPath: z.string().min(1),
  characterEntityId: z.string().min(1),
  visualSpec: z.object({
    head: z.string().min(1),
    eyes: z.string().min(1),
    brows: z.string().min(1),
    nose: z.string().min(1),
    lips: z.string().min(1),
    skin: z.string().min(1),
    hair: z.string().min(1),
    build: z.string().min(1),
    expression: z.string().min(1)
  }),
  baselineWardrobe: z.string().min(1)
});
```

The runtime must not silently synthesize missing anatomy from sparse Canon.

---

# 8. Prerequisites

Use a closed discriminated union:

```ts
export type Prerequisite =
  | {
      type: "canon_entity_exists";
      entityType: CanonEntityType;
      inputRef: string;
    }
  | {
      type: "canon_section_locked";
      entityInputRef: string;
      sectionKey: string;
    }
  | {
      type: "canonical_asset_exists";
      ownerEntityInputRef: string;
      assetType: AssetType;
    }
  | {
      type: "asset_version_status";
      assetVersionInputRef: string;
      status: AssetVersionStatus;
    };
```

For `character.create_face_lock`:

```ts
[
  {
    type: "canon_entity_exists",
    entityType: "character",
    inputRef: "characterEntityId"
  }
]
```

Locked `visual_summary`, `role_tag`, and `visual_locks` are optional context, not launch prerequisites.

Prerequisite report:

```ts
export interface PrerequisiteCheck {
  id: string;
  prerequisite: Prerequisite;
  status: "pass" | "fail";
  message: string;
  resolvedRef: string | null;
}

export interface PrerequisiteReport {
  passed: boolean;
  checks: PrerequisiteCheck[];
}
```

---

# 9. Protected TBD Guards

```ts
export type TbdGuard =
  | {
      type: "entity_scope";
      entityInputRef: string;
    }
  | {
      type: "section_scope";
      entityInputRef: string;
      sectionKey: string;
    }
  | {
      type: "project_scope";
    };
```

Face Lock has:

```ts
tbdGuards: []
```

The runtime still implements generic guard evaluation for later operations.

```ts
export interface TbdGuardReport {
  blocked: boolean;
  matchingTbds: CanonTbd[];
}
```

If blocked, no workflow run is created.

---

# 10. Workflow Step Types

Use exactly:

```text
validate_input
resolve_context
compile_request
approval
execute
complete
```

Typed definitions:

```ts
export type WorkflowStepDefinition =
  | ValidateInputStep
  | ResolveContextStep
  | CompileRequestStep
  | ApprovalStep
  | ExecuteStep
  | CompleteStep;
```

No generic script/function step.

---

# 11. Character Face Lock Workflow Definition

```ts
workflow: [
  {
    id: "validate-input",
    type: "validate_input"
  },
  {
    id: "resolve-context",
    type: "resolve_context",
    resolverId: "character_face_lock_context"
  },
  {
    id: "compile-request",
    type: "compile_request",
    compilerId: "character_face_lock_v1"
  },
  {
    id: "approve-request",
    type: "approval",
    title: "Approve Face Lock Request",
    description:
      "Review canonical context and compiled generation request before execution.",
    approvalArtifactRef: "compiled_request"
  },
  {
    id: "execute",
    type: "execute",
    executorKind: "dry_run",
    requestArtifactRef: "compiled_request"
  },
  {
    id: "complete",
    type: "complete"
  }
]
```

---

# 12. Run and Step States

Workflow run status:

```text
created
running
waiting_for_approval
ready_for_execution
completed
rejected
cancelled
failed
```

Workflow step status:

```text
pending
running
waiting
completed
skipped
failed
```

Rules:
- approval step = `waiting` while run = `waiting_for_approval`;
- approve → run `ready_for_execution`;
- approval does not execute;
- rejected/cancelled/failed/completed are terminal.

---

# 13. Workflow Events

```ts
export type WorkflowEventType =
  | "run_created"
  | "run_started"
  | "step_started"
  | "step_completed"
  | "approval_requested"
  | "approval_granted"
  | "approval_rejected"
  | "execution_started"
  | "execution_completed"
  | "run_completed"
  | "run_cancelled"
  | "run_failed";
```

Every event has contiguous run-scoped sequence number.

---

# 14. Workflow Context Snapshot

```ts
export interface WorkflowContextSnapshot {
  snapshotVersion: 1;

  project: {
    projectId: string;
  };

  skill: {
    skillId: string;
    skillVersion: string;
    operationId: string;
  };

  input: unknown;
  prerequisiteReport: PrerequisiteReport;
  canon: CanonSnapshotRef[];
  assets: AssetSnapshotRef[];
  protectedTbds: CanonTbdSnapshot[];
  resolvedContext: unknown;
  capturedAt: string;
}
```

Canon snapshot stores:
- entity ID/type;
- section ID/key;
- exact revision;
- status locked;
- exact value.

Asset snapshot stores:
- asset ID;
- asset version ID;
- asset type;
- version number;
- status canonical;
- path.

Historical context must remain understandable after current Canon changes.

---

# 15. Face Lock Context Resolver

Resolver:

```text
character_face_lock_context
```

Resolve locked Character sections if present:

```text
role_tag
visual_summary
visual_locks
```

Ignore draft sections.

Resolved context:

```ts
export interface CharacterFaceLockResolvedContext {
  character: {
    entityId: string;
    storyName: string;
    roleTag: string | null;
    visualSummary: string | null;
    permanentVisualLocks: VisualLock[];
  };

  detailedVisualSpec: {
    head: string;
    eyes: string;
    brows: string;
    nose: string;
    lips: string;
    skin: string;
    hair: string;
    build: string;
    expression: string;
  };

  baselineWardrobe: string;

  referencePlateRules: {
    background: "flat 18% neutral gray field";
    lighting: "flat shadowless neutral illumination";
    castShadow: false;
    contactShadow: false;
    cinematicDepthOfField: false;
    biologicalRealism: true;
  };
}
```

---

# 16. Provider-Neutral Execution Request

```ts
export interface ExecutionRequest {
  requestVersion: 1;

  task:
    | "character_face_lock"
    | "character_outfit"
    | "character_sheet"
    | "world_plate"
    | "shot_keyframe";

  mediaType: "image";
  prompt: string;
  references: ExecutionReference[];
  constraints: ExecutionConstraint[];
  expectedOutput: ExpectedOutputDefinition;

  provenance: {
    workflowRunId: string;
    skillId: string;
    skillVersion: string;
    operationId: string;
  };
}
```

P3 emits only:

```text
task = character_face_lock
```

There is intentionally no provider/model/API field.

---

# 17. Structured Execution Constraints

```ts
export type ExecutionConstraint =
  | {
      type: "flat_reference_background";
      value: "18_percent_neutral_gray";
    }
  | {
      type: "shadowless_lighting";
      value: true;
    }
  | {
      type: "no_cast_shadow";
      value: true;
    }
  | {
      type: "no_contact_shadow";
      value: true;
    }
  | {
      type: "no_cinematic_dof";
      value: true;
    }
  | {
      type: "preserve_visual_lock";
      key: string;
      description: string;
    };
```

---

# 18. Expected Output

```ts
export interface ExpectedOutputDefinition {
  assetType: AssetType;
  mediaType: "image" | "video" | "audio";
  desiredStatus: "candidate";
  ownerEntityInputRef: string | null;
}
```

Face Lock:

```ts
{
  assetType: "face_lock",
  mediaType: "image",
  desiredStatus: "candidate",
  ownerEntityInputRef: "characterEntityId"
}
```

---

# 19. Deterministic Compiler

```rust
pub trait RequestCompiler {
    fn id(&self) -> &'static str;

    fn compile(
        &self,
        workflow_run_id: &str,
        skill: &SkillDefinition,
        operation: &SkillOperation,
        context: &WorkflowContextSnapshot,
    ) -> Result<ExecutionRequest, AppError>;
}
```

Compiler registry includes:

```text
character_face_lock_v1
```

Same snapshot must produce same request and prompt bytes.

Prompt order:

```text
TASK
VISUAL SPEC
LOCKED BIBLE-LEVEL VISUAL CONTEXT
PERMANENT VISUAL LOCKS
BASELINE WARDROBE
POSE / EXPRESSION
REFERENCE PLATE RULES
BIOLOGICAL REALISM
FORBIDDEN STYLIZATION
OUTPUT INTENT
```

Rules:
- proper Character story name is metadata, not visual identity instruction;
- no provider syntax;
- no cinematic lighting;
- no accidental TBD resolution.

---

# 20. DryRun Executor

```rust
pub trait ExecutionExecutor {
    fn kind(&self) -> &'static str;

    fn execute(
        &self,
        request: &ExecutionRequest,
        output_dir: &Path,
    ) -> Result<ExecutionResult, AppError>;
}
```

P3 executor:

```text
DryRunExecutor
kind = dry_run
```

Artifacts:

```text
<project-root>/workflows/<run-ulid>/
├── context-snapshot.json
├── compiled-request.json
├── compiled-prompt.txt
└── dry-run-result.json
```

No provider call. No generated media asset.

---

# 21. SQLite Migration 0004

Create `apps/desktop/src-tauri/migrations/0004_workflow_runtime.sql`.

```sql
CREATE TABLE workflow_runs (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  skill_id TEXT NOT NULL,
  skill_version TEXT NOT NULL,
  operation_id TEXT NOT NULL,
  status TEXT NOT NULL CHECK (
    status IN (
      'created',
      'running',
      'waiting_for_approval',
      'ready_for_execution',
      'completed',
      'rejected',
      'cancelled',
      'failed'
    )
  ),
  input_json TEXT NOT NULL,
  prerequisite_report_json TEXT,
  context_snapshot_json TEXT,
  current_step_index INTEGER NOT NULL DEFAULT 0,
  failure_code TEXT,
  failure_message TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  completed_at TEXT,
  FOREIGN KEY (project_id) REFERENCES projects(id)
);

CREATE INDEX idx_workflow_runs_project
  ON workflow_runs(project_id, created_at DESC);

CREATE TABLE workflow_steps (
  id TEXT PRIMARY KEY,
  workflow_run_id TEXT NOT NULL,
  step_definition_id TEXT NOT NULL,
  step_index INTEGER NOT NULL,
  step_type TEXT NOT NULL CHECK (
    step_type IN (
      'validate_input',
      'resolve_context',
      'compile_request',
      'approval',
      'execute',
      'complete'
    )
  ),
  status TEXT NOT NULL CHECK (
    status IN (
      'pending',
      'running',
      'waiting',
      'completed',
      'skipped',
      'failed'
    )
  ),
  input_json TEXT,
  output_json TEXT,
  started_at TEXT,
  completed_at TEXT,
  FOREIGN KEY (workflow_run_id) REFERENCES workflow_runs(id),
  UNIQUE(workflow_run_id, step_index),
  UNIQUE(workflow_run_id, step_definition_id)
);

CREATE TABLE workflow_events (
  id TEXT PRIMARY KEY,
  workflow_run_id TEXT NOT NULL,
  sequence INTEGER NOT NULL CHECK (sequence > 0),
  type TEXT NOT NULL CHECK (
    type IN (
      'run_created',
      'run_started',
      'step_started',
      'step_completed',
      'approval_requested',
      'approval_granted',
      'approval_rejected',
      'execution_started',
      'execution_completed',
      'run_completed',
      'run_cancelled',
      'run_failed'
    )
  ),
  step_definition_id TEXT,
  payload_json TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY (workflow_run_id) REFERENCES workflow_runs(id),
  UNIQUE(workflow_run_id, sequence)
);

CREATE TABLE workflow_approvals (
  id TEXT PRIMARY KEY,
  workflow_run_id TEXT NOT NULL,
  step_definition_id TEXT NOT NULL,
  decision TEXT NOT NULL CHECK (
    decision IN ('approved', 'rejected')
  ),
  artifact_json TEXT NOT NULL,
  note TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY (workflow_run_id) REFERENCES workflow_runs(id),
  UNIQUE(workflow_run_id, step_definition_id)
);
```

---

# 22. Skill Registry

```rust
pub struct SkillRegistry {
    skills: HashMap<String, SkillDefinition>,
}
```

Key:

```text
<skill-id>@<version>
```

Methods:

```rust
pub fn builtin() -> Result<Self, AppError>;
pub fn get(&self, skill_id: &str, version: &str) -> Result<&SkillDefinition, AppError>;
pub fn list(&self) -> Vec<&SkillDefinition>;
pub fn find_operation(
    &self,
    skill_id: &str,
    version: &str,
    operation_id: &str,
) -> Result<(&SkillDefinition, &SkillOperation), AppError>;
```

Do not scan untrusted skill files in P3.

---

# 23. Workflow Runtime API

```rust
pub struct WorkflowRuntime;
```

```rust
pub fn create_run(
    project_root: &Path,
    skill_id: &str,
    skill_version: &str,
    operation_id: &str,
    input: serde_json::Value,
) -> Result<WorkflowRunDto, AppError>;

pub fn advance_run(
    project_root: &Path,
    workflow_run_id: &str,
) -> Result<WorkflowRunDetailDto, AppError>;

pub fn approve_run_step(
    project_root: &Path,
    workflow_run_id: &str,
    step_definition_id: &str,
    note: Option<String>,
) -> Result<WorkflowRunDetailDto, AppError>;

pub fn reject_run_step(
    project_root: &Path,
    workflow_run_id: &str,
    step_definition_id: &str,
    note: Option<String>,
) -> Result<WorkflowRunDetailDto, AppError>;

pub fn cancel_run(
    project_root: &Path,
    workflow_run_id: &str,
) -> Result<WorkflowRunDetailDto, AppError>;

pub fn get_run(
    project_root: &Path,
    workflow_run_id: &str,
) -> Result<WorkflowRunDetailDto, AppError>;

pub fn list_runs(
    project_root: &Path,
) -> Result<Vec<WorkflowRunSummaryDto>, AppError>;
```

---

# 24. Runtime Semantics

## Create
- validate definition and input;
- evaluate prerequisites;
- evaluate TBD guards;
- if blocked/fail: create no run;
- persist run + all pending steps + `run_created`.

## Advance
Execute deterministic steps until:
- approval gate;
- ready-for-execution boundary;
- terminal state;
- failure.

## Approval
Approve changes only to `ready_for_execution`.

It does not execute.

## Execute
Only explicit `advance_run` from ready state executes DryRun.

## Reject
Terminal. Remaining steps skipped.

## Cancel
Terminal. Remaining pending steps skipped.

## Restart
Never auto-advance.

## Interrupted `running`
Recovery marks failed with `INTERRUPTED_DURING_STEP`; never replays.

---

# 25. Error Additions

Stable error codes for:

```text
SkillNotFound
SkillVersionNotFound
SkillOperationNotFound
InvalidBuiltinSkillDefinition
WorkflowInputInvalid
WorkflowPrerequisiteFailed
WorkflowBlockedByProtectedTbd
WorkflowRunNotFound
WorkflowStepNotFound
WorkflowInvalidTransition
WorkflowApprovalRequired
WorkflowApprovalAlreadyDecided
WorkflowRunTerminal
WorkflowRunInconsistent
WorkflowCompilerNotFound
WorkflowResolverNotFound
WorkflowExecutorNotFound
WorkflowArtifactWriteFailed
WorkflowArtifactReadFailed
InterruptedDuringStep
```

---

# 26. UI

Project tabs:

```text
Assets
Canon
Workflows
```

Workflows:

```text
Available Operations
Recent Runs
```

Catalog:

```text
Character Builder
└── Create Face Lock
```

Face Lock form:
- Character
- Head
- Eyes
- Brows
- Nose
- Lips
- Skin
- Hair
- Build
- Expression
- Baseline Wardrobe

Show locked Canon context read-only.

Workflow run shows:
- skill/version;
- operation;
- status;
- ordered steps;
- snapshot;
- prompt;
- constraints;
- approval;
- dry-run result;
- event history.

---

# 27. File Structure Additions

```text
packages/domain/src/
├── skill.ts
├── skill.test.ts
├── workflow.ts
├── workflow.test.ts
├── execution.ts
└── execution.test.ts

apps/desktop/src-tauri/
├── migrations/
│   └── 0004_workflow_runtime.sql
└── src/
    ├── skills/
    │   ├── mod.rs
    │   ├── model.rs
    │   ├── registry.rs
    │   ├── validation.rs
    │   └── builtin/
    │       ├── mod.rs
    │       └── character_builder.rs
    └── workflow/
        ├── mod.rs
        ├── model.rs
        ├── repository.rs
        ├── events.rs
        ├── prerequisites.rs
        ├── tbd_guard.rs
        ├── context.rs
        ├── compiler.rs
        ├── executor.rs
        ├── artifacts.rs
        ├── runtime.rs
        ├── recovery.rs
        └── commands.rs

apps/desktop/src/features/workflows/
├── api.ts
├── WorkflowWorkspace.tsx
├── WorkflowWorkspace.test.tsx
├── OperationCatalog.tsx
├── OperationCatalog.test.tsx
├── CreateFaceLockForm.tsx
├── CreateFaceLockForm.test.tsx
├── WorkflowRunView.tsx
├── WorkflowRunView.test.tsx
├── WorkflowStepList.tsx
├── WorkflowApprovalPanel.tsx
├── WorkflowApprovalPanel.test.tsx
├── WorkflowEventTimeline.tsx
├── WorkflowArtifactViewer.tsx
└── RecentWorkflowRuns.tsx
```

---

# 28. Task Plan

## Task 1 — Add Skill / Workflow / Execution schemas and migration

- Create shared TS domain files and tests.
- Add strict Zod discriminated unions.
- Add equivalent Rust serde models/tests.
- Add `0004_workflow_runtime.sql`.
- Extend AppError.
- Verify migrations 0001–0004 apply in order.

Verification:

```bash
pnpm --filter @cinematic/domain test
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
```

Commit:

```bash
git commit -m "feat: add skill workflow runtime contracts"
```

**Acceptance:** strict contracts exist and P2 projects migrate cleanly.

---

## Task 2 — Implement builtin Skill Registry

- Write failing registry tests.
- Implement `SkillRegistry`.
- Validate semver/IDs/step graph/registries.
- Add `character-builder@1.0.0`.
- Add `character.create_face_lock`.
- Snapshot-test serialized definition.

Commit:

```bash
git commit -m "feat: register character builder workflow"
```

**Acceptance:** exact skill version + operation resolves deterministically.

---

## Task 3 — Implement workflow persistence and events

- Write create-run persistence test.
- Implement transactional run + step creation.
- Implement contiguous event sequence.
- Implement run/detail queries.
- Add project isolation tests.

Commit:

```bash
git commit -m "feat: persist workflow runs and events"
```

**Acceptance:** run/steps/events are durable and project-scoped.

---

## Task 4 — Implement prerequisites and TBD guards

- Character existence/type/project tests.
- Implement prerequisite evaluator.
- Add canonical-asset prerequisite tests proving `newest != canonical`.
- Implement canonical-asset prerequisite resolver.
- Add protected TBD guard tests.
- Implement guard evaluator.

Commit:

```bash
git commit -m "feat: validate workflow prerequisites"
```

**Acceptance:** launch eligibility is deterministic and explainable.

---

## Task 5 — Implement immutable context snapshots

- Build fixture with locked Role Tag/Visual Summary/Visual Locks and draft Psychology.
- Write failing resolver test.
- Implement resolver registry.
- Implement `character_face_lock_context`.
- Store exact section IDs/revisions/values.
- Implement atomic artifact writes.
- Test snapshot remains unchanged after current Canon mutation.

Commit:

```bash
git commit -m "feat: snapshot workflow canon context"
```

**Acceptance:** workflow context cannot drift.

---

## Task 6 — Implement request compiler and DryRun

- Write exact face-lock request snapshot test.
- Assert no provider/model names.
- Assert visual locks/neutral-reference constraints.
- Assert proper story name not used as visual identity instruction.
- Implement compiler registry and `character_face_lock_v1`.
- Compile twice; assert byte equality.
- Implement DryRun executor.
- Assert no media Asset generated.

Commit:

```bash
git commit -m "feat: compile provider neutral workflow requests"
```

**Acceptance:** same snapshot deterministically produces a provider-independent execution envelope.

---

## Task 7 — Implement WorkflowRuntime state machine

- Write create-run tests.
- Implement launch semantics.
- Write advance-to-approval test.
- Implement deterministic advance loop.
- Implement approval/rejection.
- Prove approval does not execute.
- Implement explicit DryRun execution.
- Implement cancellation.
- Test restart at approval.
- Test restart at ready state.
- Implement interrupted-running recovery → failed, never replay.

Commit:

```bash
git commit -m "feat: execute resumable skill workflows"
```

**Acceptance:** workflow pauses, resumes and completes without hidden transitions.

---

## Task 8 — Expose Tauri API and operation-launch UI

Commands:

```text
list_skill_operations
create_workflow_run
advance_workflow_run
approve_workflow_step
reject_workflow_step
cancel_workflow_run
get_workflow_run
list_workflow_runs
```

- Add typed frontend wrappers.
- Add Workflows project tab.
- Build Operation Catalog.
- Build Create Face Lock form.
- Show locked Canon context read-only.
- Label draft Canon as excluded.
- Create run then advance to approval.

Commit:

```bash
git commit -m "feat: launch character builder workflows"
```

**Acceptance:** user launches Face Lock explicitly from desktop.

---

## Task 9 — Build run/approval/artifact/history UI

- Implement ordered step view.
- Implement approval panel.
- Show snapshot, prompt, constraints.
- Approve / Reject / Cancel.
- After approval, separate `Execute Dry Run`.
- Implement safe artifact viewer.
- Implement event timeline.
- Implement recent workflow runs.
- Test rendering does not auto-advance persisted run.

Commit:

```bash
git commit -m "feat: add workflow approval and history UI"
```

**Acceptance:** user can inspect and control every runtime transition.

---

## Task 10 — End-to-end P3 acceptance

Create `workflow_runtime_acceptance.rs`.

Scenario:

1. Create `Red Door`.
2. Create/lock Mara Canon.
3. Launch Face Lock.
4. Verify locked Canon only.
5. Advance to approval.
6. Mutate current Canon.
7. Verify historical snapshot unchanged.
8. Restart at approval.
9. Verify no execution.
10. Approve.
11. Verify ready but no execution.
12. Restart at ready.
13. Verify no execution.
14. Execute DryRun.
15. Verify completed.
16. Verify artifacts.
17. Verify no provider/model fields.
18. Verify no media Asset version created.
19. Verify event sequence.
20. Launch second run and reject.
21. Verify remaining steps skipped.
22. Add test-only guarded operation proving protected TBD blocks launch.

Run:

```bash
pnpm install
pnpm test
pnpm test:rust
pnpm --filter @cinematic/desktop tauri build --debug
```

Commit:

```bash
git commit -m "test: verify workflow runtime end to end"
```

**Acceptance:** Face Lock is versioned, prerequisite-aware, snapshot-based, approval-gated, restart-safe, provider-neutral and fully auditable.

---

# 29. Manual Verification Checklist

1. Open `Red Door`.
2. Confirm Mara exists and Visual Locks are LOCKED.
3. Open Workflows.
4. Select Character Builder → Create Face Lock.
5. Fill detailed visual specification.
6. Submit.
7. Confirm Approval Required.
8. Inspect Canon Snapshot.
9. Confirm only locked Canon appears.
10. Inspect prompt.
11. Confirm no provider/model name.
12. Confirm eyebrow-scar lock appears.
13. Close app.
14. Reopen.
15. Confirm still waiting for approval.
16. Approve.
17. Confirm Ready for Execution.
18. Confirm no execution happened.
19. Close/reopen.
20. Confirm still ready.
21. Execute Dry Run.
22. Confirm completed.
23. Inspect all four workflow artifact files.
24. Confirm no image generated.
25. Confirm no Asset version created.
26. Inspect event history and skill version.
27. Launch another run.
28. Reject at approval.
29. Confirm execute/complete skipped.

---

# 30. Runtime Invariant Matrix

| Invariant | Enforcement |
|---|---|
| Skill ID/version immutable | Registry |
| Exact operation/version resolution | Registry |
| Invalid input cannot launch | Input schema |
| Missing prerequisite cannot launch | Prerequisite evaluator |
| Relevant protected TBD blocks | TBD guard |
| Draft Canon excluded | Context resolver |
| Noncanonical Asset excluded | Authority resolver |
| Context immutable after launch | Snapshot |
| Approval explicit | State machine |
| Approval does not execute | Ready state |
| Restart never auto-runs | Persistence/UI tests |
| Interrupted work not replayed | Recovery |
| Provider names absent | ExecutionRequest |
| Skill version visible historically | Run metadata |
| Events append-only | workflow_events |
| Assets untouched in P3 | Acceptance |
| Canon not mutated by runtime | Service boundaries |

---

# 31. P3 Definition of Done

P3 is done only when:

1. Strict versioned SkillDefinition exists.
2. Invalid builtin skill fails startup/test validation.
3. `character-builder@1.0.0` exists.
4. `character.create_face_lock` exists.
5. User can browse operations.
6. User can select Character and enter visual spec.
7. Invalid input cannot create a run.
8. Missing prerequisites cannot create a run.
9. Prerequisite report persists.
10. Canonical-asset prerequisites respect canonical state.
11. Protected-TBD guard works.
12. Run stores skill/version/operation.
13. Ordered steps persist.
14. Events append-only.
15. Context includes only locked Canon.
16. Snapshot stores exact revisions/values.
17. Snapshot survives later Canon changes.
18. Request is provider-neutral.
19. Compiler deterministic.
20. Prompt contains permanent locks and neutral reference rules.
21. Workflow pauses for approval.
22. Restart at approval does not advance.
23. Approving does not execute.
24. Restart after approval does not execute.
25. User explicitly triggers DryRun.
26. DryRun makes no network call.
27. DryRun creates no media Asset.
28. Completed/rejected/cancelled state survives restart.
29. Interrupted running work is failed, not replayed.
30. UI exposes steps, approval, artifacts, events and skill version.
31. P0/P2 tests still pass.
32. TS/Rust/P3 acceptance tests pass.
33. Tauri debug build succeeds.
34. Manual verification passes.

---

# 32. What Must Not Leak Into P3

Reject:
- Gemini/OpenAI/Fal/Replicate SDKs
- ComfyUI API
- Runway/PixVerse
- provider/model fields in ExecutionRequest
- image/video generation
- VLM QA
- image similarity
- automatic generated Asset creation
- natural-language routing
- LLM intent classification
- user-executable skill scripts
- plugin marketplace

---

# 33. Future P4 Boundary

P4 should add:

```text
ExecutionRequest
      ↓
Provider Router
      ↓
Provider Adapter
      ↓
Provider Result
      ↓
Asset Candidate Version
```

without rewriting:
- SkillDefinition;
- Canon Engine;
- WorkflowRuntime state machine;
- approval semantics;
- historical run semantics.

Central P4 architecture test:

> The same `character.create_face_lock` skill can switch from DryRun to one real image provider without changing the skill definition or workflow transition model.

---

# 34. Recommended Commit Sequence

```text
feat: add skill workflow runtime contracts
feat: register character builder workflow
feat: persist workflow runs and events
feat: validate workflow prerequisites
feat: snapshot workflow canon context
feat: compile provider neutral workflow requests
feat: execute resumable skill workflows
feat: launch character builder workflows
feat: add workflow approval and history UI
test: verify workflow runtime end to end
```

---

# 35. Self-Review

## Spec coverage

Covered:
- SkillDefinition
- Skill Registry
- operations
- prerequisites
- TBD guards
- state machine
- persistence
- immutable snapshots
- approval
- restart behavior
- deterministic request compiler
- DryRun
- one real Character Builder workflow
- UI
- event history
- acceptance testing

No P3 requirement is deferred into P4.

## Placeholder scan

The plan specifies concrete files, contracts, state values, SQL, transition rules, test scenarios, commands and commit boundaries. No vague implementation placeholder remains.

## Type/signature consistency

Checked:
- skill = `character-builder@1.0.0`
- operation = `character.create_face_lock`
- run/step/event states align with SQL
- canonical context = locked Canon + canonical Assets only
- ExecutionRequest has no provider/model fields
- output contract = `face_lock / image / candidate`
- approval and execution are separate
- DryRun never creates media
- P4 can reuse the execute step unchanged

---

# 36. Execution Handoff

When implementing:

1. Use isolated git worktree.
2. Prefer `superpowers:subagent-driven-development`.
3. One fresh implementation subagent per task.
4. Run task tests before review.
5. Review against task acceptance and Runtime Invariant Matrix.
6. Commit before next task.
7. Keep provider code out of P3.

After P3 passes:

> **STOP.**

Create a new `writing-plans` plan for:

# P4 — Provider Adapter Layer

P4 planning must consume the actual implemented:
- `ExecutionRequest`
- `ExecutionExecutor`
- Workflow execute-step semantics
- `ExpectedOutputDefinition`
- workflow artifact storage
- AssetService candidate-version APIs
