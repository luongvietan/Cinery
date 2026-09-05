# Joey Cinema Sequence-First Flow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn existing Cinery scenes into a sequence-first cinematic workflow with explicit director approvals, persistent state, guarded generation/review/extension stages, and a non-autonomous AI co-director rail.

**Architecture:** Reuse the existing Scene, scene-reference, shot, cinema compilation, and candidate-review foundations as the canonical production data. Add a small persisted sequence-flow record keyed by `scene_id` for the human-authored brief and the workflow’s explicit approval state. Build the experience inside `SceneWorkspace`, retaining its Setup/Shots/Render capabilities but presenting them as ordered sequence stages; the right rail consumes read-only stage context and emits only user-applied suggestions.

**Tech Stack:** React 18 + TypeScript + Vitest/Testing Library; Tauri 2 commands; Rust + rusqlite migrations and integration tests; Zod domain types; existing provider/workflow and cinema review services.

**Spec:** `docs/superpowers/specs/2026-09-05-joey-sequence-flow-design.md`

## Global Constraints

- A Scene is the sequence aggregate; do not introduce a second creative aggregate or duplicate scene/reference/shot data.
- The creator must explicitly approve every mutation that changes the brief, references, prompt, generation, canonical take, or extension direction.
- AI co-director output is read-only guidance; it must not call a mutating Tauri command, start a workflow, or spend credits.
- Preserve failed/cancelled workflow inputs and prior video candidates; no automatic retry.
- Generation preflight must show model/settings, all references, complete prompt, runtime rule result, and estimated credit impact before the mutation action is enabled.
- Runtime validation must obey the currently supported cinema bounds of 1–120 seconds; Joey’s recommended ≤15-second prompt-unit guidance is presented as a split recommendation, not a silent truncation.
- Extension is allowed only from the exact canonical video pinned to a shot and requires an explicit `prequel` or `sequel` direction.
- Follow existing `invokeCommand`, `describeError`, optimistic-concurrency, and test-fixture conventions.

---

## File Structure

| Path | Responsibility |
| --- | --- |
| `apps/desktop/src-tauri/migrations/0025_sequence_flow.sql` | Persists one sequence-flow state record per scene. |
| `packages/domain/src/sequence-flow.ts` | Shared Zod schemas and TypeScript types for stages, briefs, preflight, and extension requests. |
| `packages/domain/src/sequence-flow.test.ts` | Domain validation tests. |
| `apps/desktop/src-tauri/src/cinema/sequence_flow.rs` | Repository/service operations enforcing valid stage transitions and canonical-video extension gating. |
| `apps/desktop/src-tauri/src/cinema/commands.rs` | Tauri commands that expose sequence-flow reads and explicit mutations. |
| `apps/desktop/src-tauri/tests/sequence_flow.rs` | Rust command/service acceptance coverage. |
| `apps/desktop/src/features/scenes/sequenceFlowApi.ts` | Typed frontend command facade. |
| `apps/desktop/src/features/scenes/SequenceBrief.tsx` | Human-authored director brief and explicit lock action. |
| `apps/desktop/src/features/scenes/SequencePreflight.tsx` | Read-only generation disclosure and user approval control. |
| `apps/desktop/src/features/scenes/SequenceExtend.tsx` | Canonical-take-only extension direction and disclosure. |
| `apps/desktop/src/features/scenes/AiCoDirectorRail.tsx` | Persistent, read-only contextual checklist and optional suggestions. |
| `apps/desktop/src/features/scenes/SceneWorkspace.tsx` | Ordered stage shell, stage status, main canvas, and right rail composition. |
| `apps/desktop/src/features/scenes/*.test.tsx` | Focused component and workflow tests. |
| `apps/desktop/src/features/projects/ProjectWorkspace.tsx` | Rename navigation copy from “Scenes” to “Sequences” while retaining stable panel id `scenes`. |
| `apps/desktop/src/features/projects/ProjectWorkspace.test.tsx` | Navigation copy/regression coverage. |

## Task 1: Define the sequence-flow contract

**Files:**
- Create: `packages/domain/src/sequence-flow.ts`
- Create: `packages/domain/src/sequence-flow.test.ts`
- Modify: `packages/domain/src/index.ts`

**Interfaces:**
- Produces: `SequenceStage`, `SequenceBrief`, `SequenceFlow`, `SequencePreflight`, `ExtensionDirection`, and `ExtensionRequest`.
- Consumes: `CinemaCompilation`, `SceneDetail`, and `ShotVideoCandidate` concepts already exposed by `packages/domain/src/cinema.ts`.

- [x] **Step 1: Write failing schema tests for valid flow state and rejected invalid inputs.**

```ts
it("accepts a locked brief and rejects an empty creative intent", () => {
  expect(sequenceBriefSchema.parse({ intent: "Tay notices the door", energy: "elevated", creditCap: 800 })).toMatchObject({ creditCap: 800 });
  expect(() => sequenceBriefSchema.parse({ intent: " ", energy: "elevated", creditCap: 800 })).toThrow();
});

it("only accepts the two deliberate extension directions", () => {
  expect(extensionDirectionSchema.parse("prequel")).toBe("prequel");
  expect(() => extensionDirectionSchema.parse("continue")).toThrow();
});
```

- [x] **Step 2: Run the new test file and verify it fails because the module does not exist.**

Run: `pnpm --filter @cinematic/domain test -- sequence-flow.test.ts`

Expected: FAIL with a module-not-found or missing-export error for `sequence-flow`.

- [x] **Step 3: Implement the shared schemas and export them.**

```ts
export const sequenceStageSchema = z.enum([
  "draft", "brief_locked", "references_ready", "prompt_approved",
  "generating", "in_review", "canonical_selected", "ready_for_edit",
]);
export const extensionDirectionSchema = z.enum(["prequel", "sequel"]);
export const sequenceBriefSchema = z.object({
  intent: z.string().trim().min(1).max(1000),
  energy: z.enum(["composed", "elevated", "kinetic", "violent"]),
  targetDurationSeconds: z.number().positive().max(120),
  creditCap: z.number().int().nonnegative(),
});
```

Define `SequenceFlow` with `sceneId`, `brief`, `stage`, `approvedCompilationId`, `canonicalShotId`, `extensionDirection`, `createdAt`, and `updatedAt`. Define `SequencePreflight` with the full compilation, resolved references, `estimatedCredits`, `runtimeRecommendation`, and `canGenerate`/`blockers` fields. Re-export all public values from the domain barrel.

- [x] **Step 4: Run the domain test file and the existing cinema tests.**

Run: `pnpm --filter @cinematic/domain test -- sequence-flow.test.ts cinema.test.ts`

Expected: PASS.

- [x] **Step 5: Commit the contract.**

```bash
git add packages/domain/src/sequence-flow.ts packages/domain/src/sequence-flow.test.ts packages/domain/src/index.ts
git commit -m "feat: define sequence flow contract"
```

## Task 2: Persist explicit stage transitions and extension eligibility

**Files:**
- Create: `apps/desktop/src-tauri/migrations/0025_sequence_flow.sql`
- Create: `apps/desktop/src-tauri/src/cinema/sequence_flow.rs`
- Modify: `apps/desktop/src-tauri/src/cinema/mod.rs`
- Modify: `apps/desktop/src-tauri/src/cinema/commands.rs`
- Create: `apps/desktop/src-tauri/tests/sequence_flow.rs`

**Interfaces:**
- Consumes: `world_scenes`, scene-reference readiness, cinema compilation records, and `resolve_canonical_shot_video` behavior.
- Produces Tauri commands: `get_sequence_flow`, `update_sequence_brief`, `mark_sequence_references_ready`, `approve_sequence_preflight`, `begin_sequence_review`, and `prepare_sequence_extension`.

- [x] **Step 1: Write command-level acceptance tests before the migration or service exists.**

```rust
#[test]
fn extension_requires_a_canonical_video_and_explicit_direction() {
    let fixture = fixture();
    let scene = create_scene(&fixture);
    assert_err_contains(
        prepare_sequence_extension(fixture.root.clone(), scene.id.clone(), "prequel".into()),
        "canonical video",
    );
    promote_fixture_candidate(&fixture, &scene);
    let prepared = prepare_sequence_extension(fixture.root, scene.id, "sequel".into()).unwrap();
    assert_eq!(prepared.direction, "sequel");
}
```

Add tests that reject a stage skip (Draft → Prompt approved), preserve the locked brief after a failed generation state transition, and return blockers rather than mutating state when references are incomplete.

- [x] **Step 2: Run the Rust integration target and verify it fails.**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test sequence_flow`

Expected: FAIL because the test target and sequence-flow commands are absent.

- [x] **Step 3: Add the migration and transactional service.**

```sql
CREATE TABLE sequence_flows (
  scene_id TEXT PRIMARY KEY REFERENCES world_scenes(id) ON DELETE CASCADE,
  brief_json TEXT NOT NULL,
  stage TEXT NOT NULL CHECK (stage IN ('draft','brief_locked','references_ready','prompt_approved','generating','in_review','canonical_selected','ready_for_edit')),
  approved_compilation_id TEXT,
  canonical_shot_id TEXT,
  extension_direction TEXT CHECK (extension_direction IN ('prequel','sequel')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

Implement `SequenceFlowService` with a `transition(conn, scene_id, expected_stage, next_stage)` helper. It must use an `UPDATE ... WHERE stage = ?` compare-and-set and return a conflict error when the row changed. `prepare_extension` must resolve the shot’s exact canonical asset version and reject null; it returns a disclosure object and does not enqueue provider work.

- [x] **Step 4: Expose only explicit mutation commands and typed read models.**

```rust
#[tauri::command]
pub fn update_sequence_brief(project_root_path: String, scene_id: String, brief: SequenceBriefInput)
    -> Result<SequenceFlowRecord, AppCommandError> { /* service call */ }
```

Map Rust records to the camelCase fields consumed by the domain schema. Validate every input before writing and register the commands alongside the existing cinema command set.

- [x] **Step 5: Run focused Rust tests plus existing cinema CRUD/review tests.**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test sequence_flow --test cinema_commands_crud --test cinema_acceptance`

Expected: PASS.

- [x] **Step 6: Commit the persistence and command layer.**

```bash
git add apps/desktop/src-tauri/migrations/0025_sequence_flow.sql apps/desktop/src-tauri/src/cinema apps/desktop/src-tauri/tests/sequence_flow.rs
git commit -m "feat: persist sequence flow stages"
```

## Task 3: Add the typed frontend facade and director brief stage

**Files:**
- Create: `apps/desktop/src/features/scenes/sequenceFlowApi.ts`
- Create: `apps/desktop/src/features/scenes/sequenceFlowApi.test.ts`
- Create: `apps/desktop/src/features/scenes/SequenceBrief.tsx`
- Create: `apps/desktop/src/features/scenes/SequenceBrief.test.tsx`
- Modify: `apps/desktop/src/features/scenes/SceneWorkspace.tsx`

**Interfaces:**
- Consumes: domain `SequenceFlow`/`SequenceBrief` and Tauri commands from Task 2.
- Produces: a loaded flow state, a human-authored brief, and a refresh callback for sibling sequence stages.

- [x] **Step 1: Write failing facade and component tests.**

```tsx
it("does not lock the director brief until intent, duration, and credit cap are valid", async () => {
  render(<SequenceBrief projectRootPath="/project" sceneId="scene-1" flow={draftFlow} onChanged={vi.fn()} />);
  expect(screen.getByRole("button", { name: "Lock brief" })).toBeDisabled();
  await userEvent.type(screen.getByLabelText("Creative intent"), "A tired man hears a bell");
  await userEvent.click(screen.getByRole("button", { name: "Lock brief" }));
  expect(updateSequenceBrief).toHaveBeenCalledWith("/project", "scene-1", expect.objectContaining({ intent: "A tired man hears a bell" }));
});
```

- [x] **Step 2: Run the focused frontend tests and verify they fail.**

Run: `pnpm --filter @cinematic/desktop test -- sequenceFlowApi.test.ts SequenceBrief.test.tsx`

Expected: FAIL because the facade and component do not exist.

- [x] **Step 3: Implement `sequenceFlowApi.ts` with direct `invokeCommand` wrappers.**

```ts
export function updateSequenceBrief(root: string, sceneId: string, brief: SequenceBrief): Promise<SequenceFlow> {
  return invokeCommand<SequenceFlow>("update_sequence_brief", { projectRootPath: root, sceneId, brief });
}
```

Provide one function for each Task 2 command. Do not hide command errors or perform retries in this facade.

- [x] **Step 4: Implement the brief UI and mount it before Setup content.**

Use controlled fields for creative intent, energy, duration, and credit cap. Render the explicit “Lock brief” button only as the state mutation; use `describeError` for failures. In `SceneWorkspace`, load the flow when `selectedSceneId` changes and pass a single `handleChanged` refresh callback to the brief and subsequent stage components.

- [x] **Step 5: Run the focused tests and `SceneWorkspace` regression suite.**

Run: `pnpm --filter @cinematic/desktop test -- sequenceFlowApi.test.ts SequenceBrief.test.tsx SceneWorkspace.test.tsx`

Expected: PASS.

- [x] **Step 6: Commit the brief stage.**

```bash
git add apps/desktop/src/features/scenes/sequenceFlowApi.ts apps/desktop/src/features/scenes/sequenceFlowApi.test.ts apps/desktop/src/features/scenes/SequenceBrief.tsx apps/desktop/src/features/scenes/SequenceBrief.test.tsx apps/desktop/src/features/scenes/SceneWorkspace.tsx
git commit -m "feat: add director brief stage"
```

## Task 4: Build the generation preflight and guarded stage shell

**Files:**
- Create: `apps/desktop/src/features/scenes/SequencePreflight.tsx`
- Create: `apps/desktop/src/features/scenes/SequencePreflight.test.tsx`
- Modify: `apps/desktop/src/features/scenes/SceneWorkspace.tsx`
- Modify: `apps/desktop/src/features/scenes/SceneCompile.tsx`
- Modify: `apps/desktop/src/features/scenes/api.ts`

**Interfaces:**
- Consumes: `getCompileReadiness`, `compileCinema`, resolved references, `SequencePreflight`, and `approveSequencePreflight`.
- Produces: a reviewable disclosure that permits the existing render action only after an explicit approval state.

- [x] **Step 1: Write failing preflight tests.**

```tsx
it("shows every disclosure and prevents approval when a required reference is missing", async () => {
  render(<SequencePreflight {...blockedProps} />);
  expect(screen.getByText(/Missing scene plate/i)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Approve generation" })).toBeDisabled();
});

it("requires an explicit approval before render is enabled", async () => {
  render(<SequencePreflight {...readyProps} />);
  expect(screen.getByText(readyProps.preflight.providerPrompt)).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Approve generation" }));
  expect(approveSequencePreflight).toHaveBeenCalled();
});
```

- [x] **Step 2: Run the test and verify it fails.**

Run: `pnpm --filter @cinematic/desktop test -- SequencePreflight.test.tsx`

Expected: FAIL because `SequencePreflight` is absent.

- [x] **Step 3: Implement the disclosure and stage guard.**

Render selected provider/model settings, reference roles and immutable version ids, complete compiled prompt, total runtime, Joey short-unit recommendation, estimated credits, and blockers. `SceneCompile` must disable its existing generation CTA unless the flow stage is `prompt_approved`; it must transition to `generating` only after the user submits that CTA. A reference/shot change clears prompt approval through the Task 2 compare-and-set service rather than leaving a stale approval.

- [x] **Step 4: Run preflight, scene compile, and golden-path tests.**

Run: `pnpm --filter @cinematic/desktop test -- SequencePreflight.test.tsx SceneCompile.test.tsx SceneShots.goldenpath.test.tsx`

Expected: PASS.

- [x] **Step 5: Commit guarded generation.**

```bash
git add apps/desktop/src/features/scenes/SequencePreflight.tsx apps/desktop/src/features/scenes/SequencePreflight.test.tsx apps/desktop/src/features/scenes/SceneWorkspace.tsx apps/desktop/src/features/scenes/SceneCompile.tsx apps/desktop/src/features/scenes/api.ts
git commit -m "feat: add sequence generation preflight"
```

## Task 5: Integrate take review and extension preparation

**Files:**
- Create: `apps/desktop/src/features/scenes/SequenceExtend.tsx`
- Create: `apps/desktop/src/features/scenes/SequenceExtend.test.tsx`
- Modify: `apps/desktop/src/features/scenes/ShotVideoReview.tsx`
- Modify: `apps/desktop/src/features/scenes/ShotVideoReview.test.tsx`
- Modify: `apps/desktop/src/features/scenes/SceneWorkspace.tsx`
- Modify: `apps/desktop/src/features/scenes/sequenceFlowApi.ts`

**Interfaces:**
- Consumes: existing `promoteShotVideoCandidate`, candidate conflict handling, and Task 2 `prepareSequenceExtension`.
- Produces: `canonical_selected` flow state after successful promotion and an explicit extension disclosure with a selected direction.

- [x] **Step 1: Write failing review/extension tests.**

```tsx
it("moves to canonical-selected only after the user promotes a take", async () => {
  render(<ShotVideoReview {...props} onChanged={onChanged} />);
  await userEvent.click(screen.getByRole("button", { name: /Promote as canonical/i }));
  await userEvent.click(screen.getByRole("button", { name: "Confirm promotion" }));
  expect(markSequenceCanonicalTake).toHaveBeenCalledWith("/project", "scene-1", "shot-1");
});

it("does not allow extension without a canonical source and a direction", async () => {
  render(<SequenceExtend {...noCanonicalProps} />);
  expect(screen.getByRole("button", { name: "Prepare extension" })).toBeDisabled();
});
```

- [x] **Step 2: Run the tests and verify they fail.**

Run: `pnpm --filter @cinematic/desktop test -- ShotVideoReview.test.tsx SequenceExtend.test.tsx`

Expected: FAIL because flow-state integration and `SequenceExtend` are absent.

- [x] **Step 3: Integrate canonical promotion with the flow service.**

After `promoteShotVideoCandidate` succeeds, call the explicit flow command with the selected shot id and refresh the parent stage. Do not call it after a rejected, restored, failed, or conflicted promotion. Keep the existing candidate list and conflict copy unchanged.

- [x] **Step 4: Implement extension preparation, not autonomous provider execution.**

The panel lists the exact canonical source version, requires radio selection of “Before this clip” (`prequel`) or “After this clip” (`sequel`), displays carried scene/reference locks and continuation prompt, then enables “Prepare extension.” The button calls `prepareSequenceExtension`; it does not invoke a provider. The resulting prepared request becomes the explicit, inspectable input for the provider-specific Extend Video capability when that capability is added.

- [x] **Step 5: Run focused and backend acceptance tests.**

Run: `pnpm --filter @cinematic/desktop test -- ShotVideoReview.test.tsx SequenceExtend.test.tsx && cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test sequence_flow --test cinema_commands_crud`

Expected: PASS.

- [x] **Step 6: Commit review and extension preparation.**

```bash
git add apps/desktop/src/features/scenes/SequenceExtend.tsx apps/desktop/src/features/scenes/SequenceExtend.test.tsx apps/desktop/src/features/scenes/ShotVideoReview.tsx apps/desktop/src/features/scenes/ShotVideoReview.test.tsx apps/desktop/src/features/scenes/SceneWorkspace.tsx apps/desktop/src/features/scenes/sequenceFlowApi.ts
git commit -m "feat: add sequence review and extension preparation"
```

## Task 6: Add the persistent, non-autonomous AI co-director rail

**Files:**
- Create: `apps/desktop/src/features/scenes/AiCoDirectorRail.tsx`
- Create: `apps/desktop/src/features/scenes/AiCoDirectorRail.test.tsx`
- Modify: `apps/desktop/src/features/scenes/SceneWorkspace.tsx`
- Modify: `apps/desktop/src/features/projects/ProjectWorkspace.tsx`
- Modify: `apps/desktop/src/features/projects/ProjectWorkspace.test.tsx`

**Interfaces:**
- Consumes: `SequenceFlow`, scene compile readiness, selected workspace stage, and the existing non-mutating production-intent routing vocabulary.
- Produces: a persistent contextual rail and user-visible navigation label “Sequences”.

- [x] **Step 1: Write failing rail and navigation tests.**

```tsx
it("shows contextual checklist items but has no generate or mutation control", () => {
  render(<AiCoDirectorRail flow={draftFlow} readiness={blockedReadiness} activeStage="brief" />);
  expect(screen.getByText(/Lock a director brief/i)).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /Generate|Approve|Promote/i })).not.toBeInTheDocument();
});

it("labels the stable scenes panel as Sequences in project navigation", async () => {
  render(<ProjectWorkspace project={project} onCloseProject={vi.fn()} />);
  await userEvent.click(screen.getByRole("button", { name: "Sequences" }));
  expect(await screen.findByRole("region", { name: /Scenes workspace/i })).toBeInTheDocument();
});
```

- [x] **Step 2: Run the tests and verify they fail.**

Run: `pnpm --filter @cinematic/desktop test -- AiCoDirectorRail.test.tsx ProjectWorkspace.test.tsx`

Expected: FAIL because the rail and “Sequences” label are absent.

- [x] **Step 3: Implement the right rail and shell layout.**

`AiCoDirectorRail` derives checklists and at most three suggestions from the passed state: missing brief, missing continuity references, missing shots/prompt approval, candidate selection, or extension choice. Render suggested navigation as non-mutating deep links only. In `SceneWorkspace`, compose the main stage canvas and `<aside aria-label="AI co-director">` rail in a responsive layout; do not conditionally remove the rail while a scene is selected. Change only the ProjectWorkspace navigation label from `Scenes` to `Sequences`; keep `PanelView` and deep-link ids stable.

- [x] **Step 4: Run full desktop flow regression tests.**

Run: `pnpm --filter @cinematic/desktop test -- AiCoDirectorRail.test.tsx ProjectWorkspace.test.tsx SceneWorkspace.test.tsx SceneShots.goldenpath.test.tsx ShotVideoReview.test.tsx`

Expected: PASS.

- [x] **Step 5: Commit the workflow shell.**

```bash
git add apps/desktop/src/features/scenes/AiCoDirectorRail.tsx apps/desktop/src/features/scenes/AiCoDirectorRail.test.tsx apps/desktop/src/features/scenes/SceneWorkspace.tsx apps/desktop/src/features/projects/ProjectWorkspace.tsx apps/desktop/src/features/projects/ProjectWorkspace.test.tsx
git commit -m "feat: add persistent AI co-director rail"
```

## Task 7: Verify the complete acceptance path

**Files:**
- Create: `apps/desktop/src/__tests__/joey-sequence-flow.test.tsx`
- Modify: `apps/desktop/src-tauri/tests/sequence_flow.rs`

**Interfaces:**
- Consumes: every public contract and explicit action from Tasks 1–6.
- Produces: end-to-end confidence that the user controls the full short-sequence flow.

- [ ] **Step 1: Write the complete failing frontend acceptance journey.**

```tsx
it("guides a director from brief through a canonical take to a prepared sequel without autonomous AI actions", async () => {
  render(<SceneWorkspace projectRootPath="/project" />);
  await createAndSelectSequence("Laundromat arrival");
  await lockValidBrief();
  await attachRequiredReferences();
  await approvePreflight();
  await promoteCandidate("take-2");
  await selectExtensionDirection("sequel");
  await userEvent.click(screen.getByRole("button", { name: "Prepare extension" }));
  expect(prepareSequenceExtension).toHaveBeenCalledWith("/project", expect.any(String), "sequel");
  expect(startWorkflow).not.toHaveBeenCalled();
});
```

- [ ] **Step 2: Run the frontend and Rust acceptance targets and verify the new journey fails before wiring is complete.**

Run: `pnpm --filter @cinematic/desktop test -- joey-sequence-flow.test.tsx && cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test sequence_flow`

Expected: FAIL until the integration wiring from Tasks 1–6 is present.

- [ ] **Step 3: Wire only the missing integration seams found by the test.**

Keep fixture data explicit: one scene plate, one character look, one shot, two generated video candidates, and one promoted canonical candidate. Mock the Tauri facade at the frontend boundary; use the existing Rust fixture/database helpers at the backend boundary. Do not add sleeps, automatic retries, or live-provider calls.

- [ ] **Step 4: Run the complete verification suite.**

Run: `pnpm --filter @cinematic/domain test && pnpm --filter @cinematic/desktop test && cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`

Expected: all commands exit 0.

- [ ] **Step 5: Build the desktop application.**

Run: `pnpm --filter @cinematic/desktop build`

Expected: TypeScript and Vite build exit 0.

- [ ] **Step 6: Commit acceptance coverage.**

```bash
git add apps/desktop/src/__tests__/joey-sequence-flow.test.tsx apps/desktop/src-tauri/tests/sequence_flow.rs
git commit -m "test: cover Joey sequence flow"
```

## Plan self-review

- **Spec coverage:** Tasks 1–3 cover the sequence state and human-authored brief; Task 4 covers references, prompt, runtime and credit preflight; Task 5 covers candidate preservation, explicit canonical selection, and canonical-only extension preparation; Task 6 covers the persistent non-autonomous rail; Task 7 covers the full happy path and failure boundaries.
- **Provider boundary:** the current repository has no provider-level Extend Video capability. This plan implements the reviewed, canonical-only extension preparation/disclosure and explicitly does not fabricate a provider call. A separate provider-capability design is required before a real remote Extend Video request can spend credits.
- **Placeholder scan:** no unresolved markers, undefined interfaces, or vague testing steps remain.
- **Type consistency:** frontend and Tauri commands use the same `SequenceFlow`, `SequenceBrief`, `SequencePreflight`, and `ExtensionDirection` contracts introduced in Task 1.
