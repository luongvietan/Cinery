# Cinema Workspace CRUD Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the World/Scene/Shot/Prop/Keyframe experience so a user can assemble, validate, compile, and inspect a scene without leaving the Cinema workspace.

**Architecture:** Expand the existing Cinema repository/service/command layers with explicit relationship CRUD, transactional shot reordering, and structured readiness. Expose those commands through typed frontend functions. Replace the current thin screen with a three-region workspace: scene list, active scene editor, and canonical reference inspector. Compilation remains deterministic and only uses explicitly pinned canonical versions.

**Tech Stack:** Rust 1.77.2, Tauri 2, rusqlite; React, TypeScript, Vitest, React Testing Library; existing semantic CSS tokens.

**Spec:** `docs/superpowers/specs/2026-08-29-cinema-workspace-design.md`

## Global Constraints

- Every compile input must be an explicit version ID; never auto-restage a newer canonical version.
- Removing cast/prop relationships must not delete source assets or Canon entities.
- Shot ordering is contiguous, deterministic, and updated transactionally.
- Structured blockers must name the entity/shot and offer a stable code for UI actions.
- All mutations are scoped to the active project and reject cross-project IDs.
- Preserve existing compiler/export behavior and P8 tests.
- Reuse the current visual system; avoid a generic dashboard grid of cards.
- Preserve the dirty working tree and stage only agent-owned hunks.

---

### Task 1: Define complete Cinema DTOs and readiness contracts

**Files:**
- Modify: `packages/domain/src/cinema.ts`
- Modify: `packages/domain/src/cinema.test.ts`
- Modify: `packages/domain/src/index.ts`
- Modify: `apps/desktop/src-tauri/src/cinema/model.rs`
- Test: inline tests in `cinema/model.rs`

**Interfaces produced:**

```ts
export type CinemaBlockerCode =
  | "missing_world"
  | "missing_cast_look"
  | "missing_cast_sheet"
  | "missing_prop"
  | "missing_shot_keyframe";

export interface CinemaReadinessBlocker {
  code: CinemaBlockerCode;
  sceneId: string;
  entityId?: string;
  shotId?: string;
  message: string;
  actionTarget: "world" | "cast" | "props" | "shot";
}

export interface CinemaSceneDetail {
  scene: CinemaScene;
  worldVersion: CinemaReference | null;
  cast: CinemaCastMember[];
  props: CinemaPropReference[];
  shots: CinemaShot[];
  readiness: { ready: boolean; blockers: CinemaReadinessBlocker[] };
}
```

- [ ] **Step 1: Write failing TypeScript and Rust serialization tests**

Cover every blocker code, optional reference fields, empty scenes, ordered shots, and camelCase IPC serialization.

- [ ] **Step 2: Run RED**

Run: `pnpm --filter @cinematic/domain test -- cinema.test.ts`

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml cinema_model -- --nocapture`

- [ ] **Step 3: Implement matching Rust/TypeScript contracts**

Keep names and optionality identical across the boundary. Do not expose database-only row shapes.

- [ ] **Step 4: Run GREEN and commit**

Suggested commit: `feat: define cinema workspace contracts`

### Task 2: Complete repository CRUD and transactional shot ordering

**Files:**
- Modify: `apps/desktop/src-tauri/src/cinema/repository.rs`
- Test: `apps/desktop/src-tauri/tests/cinema_repository.rs`

**Repository methods produced:**

```rust
pub fn rename_scene(&self, project_id: &str, scene_id: &str, name: &str) -> Result<Scene, AppError>;
pub fn set_scene_world(&self, project_id: &str, scene_id: &str, version_id: Option<&str>) -> Result<(), AppError>;
pub fn update_scene_character(&self, project_id: &str, scene_id: &str, character_id: &str, look_id: Option<&str>, sheet_id: Option<&str>) -> Result<(), AppError>;
pub fn remove_scene_character(&self, project_id: &str, scene_id: &str, character_id: &str) -> Result<(), AppError>;
pub fn remove_scene_prop(&self, project_id: &str, scene_id: &str, prop_version_id: &str) -> Result<(), AppError>;
pub fn update_shot(&self, project_id: &str, shot: &ShotUpdate) -> Result<Shot, AppError>;
pub fn delete_shot(&self, project_id: &str, scene_id: &str, shot_id: &str) -> Result<(), AppError>;
pub fn reorder_shots(&self, project_id: &str, scene_id: &str, ordered_ids: &[String]) -> Result<Vec<Shot>, AppError>;
pub fn set_shot_keyframe(&self, project_id: &str, shot_id: &str, version_id: Option<&str>) -> Result<(), AppError>;
```

- [ ] **Step 1: Write failing repository tests for every mutation**

Include cross-project rejection, duplicate cast/prop prevention, relationship-only deletion, clearing optional references, and deterministic reads after reopen.

- [ ] **Step 2: Write failing reorder transaction tests**

Assert exact ID set required, no duplicates/foreign shots, contiguous positions `0..n-1`, rollback on invalid input, and stable order after database reopen.

- [ ] **Step 3: Run RED**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test cinema_repository -- --nocapture`

- [ ] **Step 4: Implement minimal SQL within one transaction per mutation**

```rust
let tx = connection.transaction()?;
for (position, shot_id) in ordered_ids.iter().enumerate() {
    tx.execute(
        "UPDATE cinema_shots SET position = ?1 WHERE id = ?2 AND scene_id = ?3",
        params![position as i64, shot_id, scene_id],
    )?;
}
tx.commit()?;
```

Use a temporary offset or a two-phase update if a unique `(scene_id, position)` constraint would collide.

- [ ] **Step 5: Run GREEN and commit**

Suggested commit: `feat: complete cinema repository mutations`

### Task 3: Add service validation, structured readiness, and commands

**Files:**
- Modify: `apps/desktop/src-tauri/src/cinema/service.rs`
- Modify: `apps/desktop/src-tauri/src/cinema/commands.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Test: `apps/desktop/src-tauri/tests/cinema_service.rs`
- Test: `apps/desktop/src-tauri/tests/cinema_commands.rs`

**Commands produced:**

```rust
rename_scene
set_scene_world
update_scene_character
remove_scene_character
remove_scene_prop
update_shot
delete_shot
reorder_shots
set_shot_keyframe
get_scene_readiness
```

- [ ] **Step 1: Write failing service tests**

Verify canonical version/project ownership for world/look/sheet/prop/keyframe, expected asset types, scene/shot ownership, blank names, invalid durations, and relationship deletion semantics.

- [ ] **Step 2: Write failing readiness tests**

Create one fixture per blocker plus a fully ready scene. Assert blocker codes, IDs, messages, deterministic ordering, and the exact transition after setting/clearing each reference.

- [ ] **Step 3: Write failing command-boundary tests**

Call public command functions and DTOs. Assert successful round trips plus stable `AppCommandError` fields for validation/not-found/conflict failures.

- [ ] **Step 4: Run RED**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test cinema_service -- --nocapture`

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test cinema_commands -- --nocapture`

- [ ] **Step 5: Implement service methods and register commands**

Have `scene_detail` return readiness with the scene graph so the UI does not reconstruct business rules.

- [ ] **Step 6: Run GREEN and existing compiler tests**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml cinema -- --nocapture`

Suggested commit: `feat: expose complete cinema command surface`

### Task 4: Add a typed frontend Cinema API

**Files:**
- Modify: `apps/desktop/src/features/cinema/api.ts`
- Create: `apps/desktop/src/features/cinema/api.test.ts`

**Functions produced:**

```ts
export const cinemaApi = {
  listScenes(projectId: string): Promise<CinemaScene[]>;
  getScene(projectId: string, sceneId: string): Promise<CinemaSceneDetail>;
  createScene(input: CreateSceneInput): Promise<CinemaScene>;
  renameScene(input: RenameSceneInput): Promise<CinemaScene>;
  setWorld(input: SetSceneWorldInput): Promise<void>;
  updateCast(input: UpdateSceneCharacterInput): Promise<void>;
  removeCast(input: RemoveSceneCharacterInput): Promise<void>;
  addProp(input: AddScenePropInput): Promise<void>;
  removeProp(input: RemoveScenePropInput): Promise<void>;
  createShot(input: CreateShotInput): Promise<CinemaShot>;
  updateShot(input: UpdateShotInput): Promise<CinemaShot>;
  deleteShot(input: DeleteShotInput): Promise<void>;
  reorderShots(input: ReorderShotsInput): Promise<CinemaShot[]>;
  setKeyframe(input: SetShotKeyframeInput): Promise<void>;
  compileScene(input: CompileSceneInput): Promise<CinemaCompilation>;
};
```

- [ ] **Step 1: Write failing invoke-shape tests for all functions**

Assert command names, camelCase payloads, return types, and propagated normalized errors.

- [ ] **Step 2: Run RED**

Run: `pnpm --filter @cinematic/desktop test -- features/cinema/api.test.ts`

- [ ] **Step 3: Implement thin typed wrappers**

Do not hide retries or fallback behavior in the API layer.

- [ ] **Step 4: Run GREEN and commit**

Suggested commit: `feat: expose typed cinema workspace API`

### Task 5: Build the three-region Cinema workspace

**Files:**
- Modify: `apps/desktop/src/features/cinema/CinemaWorkspace.tsx`
- Create: `apps/desktop/src/features/cinema/SceneList.tsx`
- Create: `apps/desktop/src/features/cinema/SceneEditor.tsx`
- Create: `apps/desktop/src/features/cinema/ReferenceInspector.tsx`
- Create: `apps/desktop/src/features/cinema/ShotList.tsx`
- Modify: `apps/desktop/src/features/cinema/CinemaWorkspace.test.tsx`
- Modify: `apps/desktop/src/styles/app.css`

**UI state contract:**

```ts
interface CinemaSelection {
  sceneId: string | null;
  inspector:
    | { kind: "world" }
    | { kind: "cast"; characterId: string }
    | { kind: "prop"; versionId: string }
    | { kind: "shot"; shotId: string }
    | null;
}
```

- [ ] **Step 1: Write failing scene lifecycle tests**

Cover empty state, create, select, rename, refresh after mutation, error recovery, and keyboard traversal of the scene list.

- [ ] **Step 2: Write failing relationship and shot tests**

Cover set/clear World, add/update/remove cast look/sheet, add/remove Prop, create/edit/delete Shot, set/clear Keyframe, reorder, and confirm the source asset remains available after relationship deletion.

- [ ] **Step 3: Run RED**

Run: `pnpm --filter @cinematic/desktop test -- CinemaWorkspace.test.tsx`

- [ ] **Step 4: Implement the three regions**

At desktop widths use `minmax(12rem, 0.7fr) minmax(30rem, 2fr) minmax(17rem, 0.9fr)`. At narrower widths, collapse the inspector below the editor and keep the scene list as a horizontally scrollable semantic list. Keep selection during mutation/refetch.

- [ ] **Step 5: Implement explicit reference selection**

The inspector lists compatible canonical versions with asset name, version, hash prefix, and status. Selection writes the exact version ID; it never follows future canonical changes automatically.

- [ ] **Step 6: Run GREEN and manual interaction checks**

Verify visible focus, labels, dialog focus return, 200% zoom, narrow layout, empty/loading/error states, and reduced motion.

Suggested commit: `feat: build complete cinema workspace`

### Task 6: Integrate readiness, compilation, and export inspection

**Files:**
- Modify: `apps/desktop/src/features/cinema/CinemaWorkspace.tsx`
- Create: `apps/desktop/src/features/cinema/CompilationPanel.tsx`
- Modify: `apps/desktop/src/features/cinema/CinemaWorkspace.test.tsx`
- Modify: `apps/desktop/src-tauri/tests/cinema_acceptance.rs`
- Modify: `apps/desktop/src-tauri/tests/cinema_export.rs`

- [ ] **Step 1: Write failing blocker-navigation tests**

Assert Compile is disabled only when structured readiness is false; each blocker focuses the correct world/cast/prop/shot control; clearing a reference restores the blocker.

- [ ] **Step 2: Write failing compilation evidence tests**

Assert successful compile displays compilation ID, output path, SHA-256, timestamp, input-version summary, and an Open Export action. Reopen and verify the same record.

- [ ] **Step 3: Run RED**

Run: `pnpm --filter @cinematic/desktop test -- CinemaWorkspace.test.tsx`

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test cinema_acceptance -- --nocapture`

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test cinema_export -- --nocapture`

- [ ] **Step 4: Implement readiness and compilation panel**

Use the backend blocker list verbatim for logic while formatting concise user copy. Do not offer compile until all exact references are pinned.

- [ ] **Step 5: Run the slice gate**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml cinema -- --nocapture`

Run: `pnpm --filter @cinematic/desktop test -- CinemaWorkspace.test.tsx`

Run: `pnpm --filter @cinematic/desktop build`

Run: `git diff --check`

- [ ] **Step 6: Commit verified owned hunks**

Suggested commit: `feat: validate and compile scenes from cinema workspace`
