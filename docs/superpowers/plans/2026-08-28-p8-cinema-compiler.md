# P8 Cinema Compiler Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement P8 Cinema Compiler — provider-neutral video prompt compilation from canonical story/character/world state with scene/shot persistence, runtime validation, TBD firewall, and prompt export.

**Architecture:** New Rust `cinema` module (model/repository/service/compiler/export + commands) + SQLite migration 0012 (scenes, scene_characters, scene_props, shots, cinema_compilations) + domain types in `packages/domain` + Tauri wiring. Compiler is standalone (not workflow-embedded) but reuses canon/assets/TBD/query patterns from existing modules. Produces deterministic provider-neutral `CinemaCompilation` JSON + export under `prompts/cinema/`.

**Tech Stack:** Rust (Tauri 2, rusqlite bundled, serde/serde_json, chrono, ulid, sha2), TypeScript (domain), SQLite WAL.

**Spec:** `ai-cinematic-production-os-master-plan.md` #28 Cinema Compiler, #41 P8, #26 Scene Model, #27 Shot Model, #10 Canon Hierarchy, #11 TBD Firewall, #28+55 Prompt Compiler, #22-25 Face/Outfit/Sheet/World rules

## Global Constraints

- Local-first, cloud-optional; no mandatory cloud account for MVP — `packages/domain/src` and `apps/desktop/src-tauri/src` remain offline-capable
- Keep provider-specific code out of domain packages — cinema compiler is provider-neutral; no `providerId`/`modelId` in prompt output
- Keep React components free of core state-machine logic — logic lives in Rust `cinema` service/compiler
- Never silently overwrite user media — export via atomic temp+rename; DB writes in transactions
- Every implementation phase must leave the app runnable — `cargo test` and `pnpm -r test` must pass after each task
- Use explicit types rather than generic JSON where domain type is known — define `CinemaCompilation`, `ShotInstruction`, etc.
- Add tests for every important domain transition — TDD for state transitions, TBD firewall, time budgeting, continuity
- Sanitize `storagePath`-style fields where applicable — cinema exports are project-relative

---

### Task 1: Migration 0012 — Scene/Shot/Cinema DB substrate

**Files:**
- Create: `apps/desktop/src-tauri/migrations/0012_cinema_compiler.sql`
- Modify: `apps/desktop/src-tauri/src/db/migrations.rs:14-58` (append migration 12)
- Test: `apps/desktop/src-tauri/src/db/migrations.rs` (add test `cinema_migration_creates_required_tables` + FK/unique checks)

**Interfaces:**
- Consumes: existing `MIGRATIONS` const
- Produces: tables `scenes`, `scene_characters`, `scene_props`, `shots`, `cinema_compilations` queryable by `cinema::repository`

- [x] **Step 1: Write failing test for migration existence**

In `src/db/migrations.rs` add test:

```rust
#[test]
fn cinema_migration_creates_required_tables() {
    let mut conn = Connection::open_in_memory().unwrap();
    run_migrations(&mut conn).unwrap();
    for table in ["scenes","scene_characters","scene_props","shots","cinema_compilations"] {
        let exists: i64 = conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1", [table], |r| r.get(0)).unwrap();
        assert_eq!(exists, 1, "table {table} should exist");
    }
}
```

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml db::migrations::tests::cinema_migration_creates_required_tables`
Expected: FAIL — table not found

- [x] **Step 2: Create migration SQL**

`migrations/0012_cinema_compiler.sql`:

```sql
CREATE TABLE scenes (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  title TEXT NOT NULL CHECK (length(trim(title)) BETWEEN 1 AND 160),
  world_asset_version_id TEXT,
  canon_notes TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (project_id) REFERENCES projects(id),
  FOREIGN KEY (world_asset_version_id) REFERENCES asset_versions(id)
);
CREATE INDEX idx_scenes_project ON scenes(project_id, created_at DESC);

CREATE TABLE scene_characters (
  scene_id TEXT NOT NULL,
  character_entity_id TEXT NOT NULL,
  look_asset_version_id TEXT NOT NULL,
  sheet_asset_version_id TEXT,
  display_order INTEGER NOT NULL CHECK (display_order >= 0),
  FOREIGN KEY (scene_id) REFERENCES scenes(id) ON DELETE CASCADE,
  FOREIGN KEY (character_entity_id) REFERENCES canon_entities(id),
  FOREIGN KEY (look_asset_version_id) REFERENCES asset_versions(id),
  FOREIGN KEY (sheet_asset_version_id) REFERENCES asset_versions(id),
  PRIMARY KEY (scene_id, character_entity_id)
);

CREATE TABLE scene_props (
  scene_id TEXT NOT NULL,
  prop_asset_version_id TEXT NOT NULL,
  display_order INTEGER NOT NULL CHECK (display_order >= 0),
  FOREIGN KEY (scene_id) REFERENCES scenes(id) ON DELETE CASCADE,
  FOREIGN KEY (prop_asset_version_id) REFERENCES asset_versions(id),
  PRIMARY KEY (scene_id, prop_asset_version_id)
);

CREATE TABLE shots (
  id TEXT PRIMARY KEY,
  scene_id TEXT NOT NULL,
  ordering INTEGER NOT NULL CHECK (ordering >= 0),
  duration_seconds REAL NOT NULL CHECK (duration_seconds > 0 AND duration_seconds <= 30),
  keyframe_asset_version_id TEXT,
  intent TEXT NOT NULL CHECK (length(trim(intent)) BETWEEN 1 AND 240),
  action TEXT,
  camera TEXT,
  generated_video_asset_version_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (scene_id) REFERENCES scenes(id) ON DELETE CASCADE,
  FOREIGN KEY (keyframe_asset_version_id) REFERENCES asset_versions(id),
  UNIQUE(scene_id, ordering)
);
CREATE INDEX idx_shots_scene ON shots(scene_id, ordering);

CREATE TABLE cinema_compilations (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  scene_id TEXT NOT NULL,
  input_json TEXT NOT NULL CHECK (json_valid(input_json)),
  compilation_json TEXT NOT NULL CHECK (json_valid(compilation_json)),
  export_path TEXT NOT NULL,
  export_sha256 TEXT NOT NULL CHECK (length(export_sha256)=64),
  created_at TEXT NOT NULL,
  FOREIGN KEY (project_id) REFERENCES projects(id),
  FOREIGN KEY (scene_id) REFERENCES scenes(id)
);
CREATE INDEX idx_cinema_compilations_scene ON cinema_compilations(scene_id, created_at DESC);
```

- [x] **Step 3: Wire migration**

In `src/db/migrations.rs` append to `MIGRATIONS`:

```rust
Migration { version: 12, sql: include_str!("../../migrations/0012_cinema_compiler.sql"), },
```

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml db::migrations::tests::cinema_migration_creates_required_tables`
Expected: PASS

- [x] **Step 5: Run all migration tests**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml db::migrations`
Expected: PASS (existing + new)

- [x] **Step 6: Commit**

```bash
git add apps/desktop/src-tauri/migrations/0012_cinema_compiler.sql apps/desktop/src-tauri/src/db/migrations.rs
git commit -m "feat: add cinema compiler DB migration (scenes/shots/compilations)"
```

---

### Task 2: Domain types — packages/domain + Rust model

**Files:**
- Create: `packages/domain/src/cinema.ts`
- Modify: `packages/domain/src/index.ts:1-15` (export cinema)
- Create: `apps/desktop/src-tauri/src/cinema/model.rs`
- Modify: `apps/desktop/src-tauri/src/cinema/mod.rs` (new module)
- Test: `packages/domain/src/cinema.test.ts` (new), `apps/desktop/src-tauri/src/cinema/model.rs` inline tests

**Interfaces:**
- Consumes: `AssetType`, canon section types
- Produces: `SceneCreateInput`, `ShotCreateInput`, `CinemaCompileInput`, `CinemaCompilation`, `ProviderNeutralCinemaPrompt`, `ShotInstruction` used by service/compiler/commands

- [x] **Step 1: Write failing domain test (TS)**

`packages/domain/src/cinema.test.ts`:

```ts
import { describe, it, expect } from "vitest";
import { cinemaCompilationSchema } from "./cinema.js";
describe("cinema compilation schema", ()=>{
  it("rejects empty shots", ()=>{
    expect(()=>cinemaCompilationSchema.parse({ id:"c1", projectId:"p", sceneId:"s", totalDurationSeconds:8, shots:[], behavioralLocks:{}, worldContinuity:{}, audioInstructions:null, providerPrompt:"x", createdAt:new Date().toISOString()} )).toThrow();
  });
  it("accepts valid 8s two-shot", ()=>{
    const v={ id:"c1", projectId:"p", sceneId:"s", totalDurationSeconds:8, shots:[{order:0,durationSeconds:4,intent:"Establish",camera:"wide",action:"stand",continuity:"keep look"}, {order:1,durationSeconds:4,intent:"Close",camera:"medium",action:"look",continuity:"keep look"}], behavioralLocks:{speech:"calm",movement:"precise",stillness:"restrained"}, worldContinuity:{plateId:"wp-v1", notes:"Station"}, providerPrompt:"CINEMA PROMPT", createdAt:"2026-08-28T00:00:00.000Z"};
    expect(cinemaCompilationSchema.parse(v).totalDurationSeconds).toBe(8);
  });
});
```

Run: `pnpm --filter @cinematic/domain test -- cinema.test.ts`
Expected: FAIL — module not found

- [x] **Step 2: Implement TS domain**

`packages/domain/src/cinema.ts`: define zod schemas:
- `shotInstructionSchema` {order, durationSeconds (positive <=30), intent (1-240), camera optional, action optional, continuity?}
- `cinemaCompilationSchema` {id, projectId, sceneId, totalDurationSeconds (1-120), shotCount derived, shots non-empty, total duration = sum shots, behavioralLocks {speech,movement,stillness?}, worldContinuity {plateId, plateAssetVersionId, description?}, continuityNotes, audioInstructions, lastFrame, providerPrompt, createdAt}
- Helpers `validateTotalDuration`, `computeTimeBudget(duration, shotCount)`

- [x] **Step 3: Run TS test passes**

Run: `pnpm --filter @cinematic/domain test`
Expected: PASS

- [x] **Step 4: Implement Rust model**

`apps/desktop/src-tauri/src/cinema/model.rs`: define structs with serde camelCase:
`SceneRecord{id, projectId, title, worldAssetVersionId, canonNotes, createdAt, updatedAt}`
`SceneCharacterRecord{sceneId, characterEntityId, lookAssetVersionId, sheetAssetVersionId, displayOrder}`
`ShotRecord{id, sceneId, ordering, durationSeconds, keyframeAssetVersionId, intent, action, camera, createdAt, updatedAt}`
`CinemaCompileInput{sceneId, totalDurationSeconds, shotCount? optional (if None auto 2)}`
`BehavioralLocks{speech, movement, stillness}` // Option<String>
`WorldContinuity{plateAssetVersionId, plateId?, description?}`
`ShotInstruction{order, durationSeconds, intent, action, camera, continuityNote, subjectLocks, lastFrame?}`
`ProviderNeutralCinemaPrompt{projectId, sceneId, compilationId, totalDurationSeconds, timeBudget, shots, behavioralLocks, worldContinuity, continuity, audioInstructions, lastFrame, providerPrompt}`
`CinemaCompilation{id, projectId, sceneId, inputJson, compilationJson, exportPath, exportSha256, createdAt}`
Add validation fns `validate_duration`.

Add inline tests mirroring TS.

- [x] **Step 5: Run Rust model tests**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml cinema::model`
Expected: PASS

- [x] **Step 6: Commit**

```bash
git add packages/domain/src/cinema.ts packages/domain/src/cinema.test.ts packages/domain/src/index.ts apps/desktop/src-tauri/src/cinema/model.rs apps/desktop/src-tauri/src/cinema/mod.rs
git commit -m "feat: add cinema domain types (TS + Rust)"
```

---

### Task 3: Cinema repository — scenes/shots CRUD + compilation persistence

**Files:**
- Create: `apps/desktop/src-tauri/src/cinema/repository.rs`
- Modify: `apps/desktop/src-tauri/src/cinema/mod.rs` (expose repository)
- Test: `apps/desktop/src-tauri/tests/cinema_repository.rs` (new integration test) + inline unit tests

**Interfaces:**
- Consumes: `SceneRecord`, `ShotRecord`, `CinemaCompilation` from model
- Produces: `create_scene`, `get_scene`, `list_scenes`, `add_scene_character`, `add_scene_prop`, `create_shot`, `list_shots`, `insert_compilation`, `get_compilation` used by service

- [x] **Step 1: Write failing repository test**

`tests/cinema_repository.rs`:

```rust
use cinematic_desktop_lib::{db, cinema::{repository, model::*}};
use tempfile::tempdir;
...
#[test]
fn creates_scene_and_shots_with_ordering_uniqueness() {
  let dir = tempdir().unwrap();
  let project = create_project(&dir);
  let scene = create_scene(&project, "Scene 001", None);
  assert!(repository::create_scene(...).is_ok());
  // duplicate ordering -> error
}
```

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test cinema_repository`
Expected: FAIL — module not found

- [x] **Step 2: Implement repository**

`src/cinema/repository.rs`:
- `create_scene(conn, record)` — insert into scenes
- `get_scene(conn, project_id, scene_id)` — join check project_id
- `list_scenes(conn, project_id)`
- `add_scene_character(conn, rec)` — insert scene_characters
- `add_scene_prop`
- `create_shot` — validate ordering unique, duration >0
- `list_shots(conn, scene_id)` ordered by ordering
- `insert_compilation(conn, rec)`
- `get_compilation(conn, id)`

All functions return `Result<_, AppError>` mapping rusqlite errors to `AppError::Database`.

- [x] **Step 3: Run repository tests**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test cinema_repository`
Expected: PASS

- [x] **Step 4: Commit**

```bash
git add apps/desktop/src-tauri/src/cinema/repository.rs apps/desktop/src-tauri/src/cinema/mod.rs apps/desktop/src-tauri/tests/cinema_repository.rs
git commit -m "feat: add cinema repository (scenes/shots/compilations)"
```

---

### Task 4: Cinema service — retrieval + validation (character behavior, world, time budget)

**Files:**
- Create: `apps/desktop/src-tauri/src/cinema/service.rs`
- Modify: `apps/desktop/src-tauri/src/cinema/mod.rs`
- Test: `apps/desktop/src-tauri/tests/cinema_service.rs` + inline in service.rs

**Interfaces:**
- Consumes: `cinema::repository`, `canon::repository`, `assets::repository`, `canon_tbds` query
- Produces: `CinemaService::create_scene`, `add_character_to_scene`, `create_shot`, `validate_scene_for_compilation`, `resolve_behavioral_locks`, `resolve_world_continuity`, `compute_time_budget`

- [x] **Step 1: Write failing service test — behavioral locks retrieval**

```rust
#[test]
fn resolves_speech_movement_stillness_from_locked_sections() {
 // setup project + character entity + locked speech/movement/stillness sections
 // call service::resolve_behavioral_locks(conn, character_id)
 // expect Ok with Some values
}
#[test]
fn blocks_compilation_when_behavior_not_locked() {
 // missing locked speech -> Err InvalidCanonSectionValue or WorkflowPrerequisiteFailed
}
```

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test cinema_service resolves_speech`
Expected: FAIL — not implemented

- [x] **Step 2: Implement behavioral retrieval**

In `service.rs`:
- `resolve_behavioral_locks(conn, character_entity_id)` -> queries `canon_sections` where `section_key IN ('speech','movement','stillness') AND status='locked'`, parses `value_json->text`, returns struct with Option<String> trimmed non-empty; strict: if any of the three not locked => return Err `AppError::WorkflowPrerequisiteFailed("character behavioral canon not locked: speech/movement/stillness")` (choose code matching P8 strict)
- `resolve_world_continuity(conn, project_id, world_asset_version_id)` -> verify asset exists, type='world_plate', status='canonical', linked to project, load its canonical asset version path; also fetch location entity if exists for description; return WorldContinuity
- `compute_time_budget(totalDurationSeconds, shotCount)` -> if shotCount None => auto: 8s => 2 shots (4s+4s), otherwise divide evenly with remainder distribution to earlier shots, ensure sum == total; validate total 1-120, each shot 0.5-30s
- `validate_scene_for_compilation(conn, scene_id)` -> checks: scene exists, has >=1 character assignment (each look_asset_version_id is canonical and current pointer; sheet optional but if present also canonical), world plate optional but if present canonical, at least one shot, TBD firewall delegate to next task but called here

- [x] **Step 3: Implement scene/shot convenience wrappers**

`create_scene`, `add_scene_character` (validates look is canonical world_plate? no, look is outfit/character_sheet), `create_shot` delegating to repo with ordering auto-increment if not supplied.

- [x] **Step 4: Run service tests**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test cinema_service`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/cinema/service.rs
git commit -m "feat: add cinema service (behavior/world/time-budget validation)"
```

---

### Task 5: TBD firewall + continuity compiler

**Files:**
- Create: `apps/desktop/src-tauri/src/cinema/tbd_guard.rs`
- Create: `apps/desktop/src-tauri/src/cinema/compiler.rs`
- Modify: `apps/desktop/src-tauri/src/cinema/mod.rs`
- Test: `apps/desktop/src-tauri/tests/cinema_tbd.rs`, `apps/desktop/src-tauri/tests/cinema_compiler.rs` + inline

**Interfaces:**
- Consumes: `resolve_behavioral_locks`, `resolve_world_continuity`, `compute_time_budget`, `list_shots`
- Produces: `check_tbd_firewall`, `compile_provider_neutral_prompt` used by compilation workflow

- [x] **Step 1: Write failing TBD test**

```rust
#[test]
fn blocks_compilation_when_protected_tbd_open_for_scene_character() {
  // create protected TBD linked to character entity
  // attempt compile -> expect Err WorkflowBlockedByProtectedTbd
}
#[test]
fn allows_compilation_when_no_protected_tbd() { ... expect Ok }
```

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test cinema_tbd`
Expected: FAIL

- [x] **Step 2: Implement TBD guard**

`tbd_guard.rs`:
- `check_tbd_firewall(conn, project_id, scene_id)` -> query `canon_tbds WHERE project_id=?1 AND protected=1 AND status='open'` then for each tbd check if `canon_entity_id` matches any scene character or scene's world? For MVP, block if ANY protected open TBD exists in project that is not resolved (strict) OR more precise: if tbd.canon_entity_id IS NULL (project_scope) => block; if matches character/world entity => block. Return `Err(AppError::WorkflowBlockedByProtectedTbd(format!("protected TBD '{}' must be resolved before cinema compilation", topic)))`. If no such TBD, Ok.

- [x] **Step 3: Write failing compiler test**

```rust
#[test]
fn compiles_8s_two_shot_with_behavior_and_world_continuity() {
  // setup: character with locked speech/movement/stillness, world_plate canonical, scene with 2 shots (4s each)
  // compile -> check totalDuration 8, shots len 2, sum durations 8, each shot has subjectLocks from visual_locks, worldContinuity plate id, providerPrompt contains speech+movement+stillness + world + shot intents, no TBD text
}
```

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test cinema_compiler`
Expected: FAIL

- [x] **Step 4: Implement compiler**

`compiler.rs`:
- `pub fn compile(project_id, scene_id, totalDurationSeconds, compilation_id, behavioralLocks, worldContinuity, shots, visualLocks, canonSnapshotRefs) -> ProviderNeutralCinemaPrompt`
- Logic:
  1. Compute timeBudget: if shots empty -> auto-generate 2 shots from totalDuration (or use shotCount)
  2. For each shot, build `ShotInstruction` with `continuityNote = "Preserve canonical look {lookId} and world plate {worldId} across shots; character placement consistent with geography"` (cross-frame continuity)
  3. Aggregate `subjectLocks` from `visual_locks` canon (like scar, skin, watch)
  4. `performance` = concat speech+movement+stillness for character
  5. `audioInstructions` = from scene canon_notes if present
  6. `lastFrame` = last shot's camera + intent + world description
  7. `providerPrompt` = template:
```
CINEMA PRODUCTION PROMPT — Provider Neutral
Project: {projectId} Scene: {sceneTitle} ({sceneId})
Runtime: {total}s across {n} shots
Time Budget: {json of shot durations}
Character Behavioral Locks:
  speech: {speech}
  movement: {movement}
  stillness: {stillness}
Visual Locks: {json}
World Continuity: plate {worldPlateId} — preserve architecture/materials/lighting baseline
Shots:
  1. [{duration}s] {intent} — camera: {camera} action: {action} continuity: {note}
...
Continuity: each shot preserves canonical look + world; no lens over-lock; geography: Main Ops Room -> Equipment Corridor -> Red Door -> [TBD unseen] (do not invent behind door)
Audio: {instructions}
Last Frame: {lastFrame}
Provenance: story bible + scene {sceneId} + compilation {id}
```
  8. Ensure no TBD topic text is interpolated (filtered)
  9. Deterministic: sort visual locks by key, sort shots by order

- [x] **Step 5: Run tests**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml cinema_tbd cinema_compiler`
Expected: PASS

- [x] **Step 6: Commit**

```bash
git add apps/desktop/src-tauri/src/cinema/tbd_guard.rs apps/desktop/src-tauri/src/cinema/compiler.rs
git commit -m "feat: add cinema TBD firewall and continuity compiler"
```

---

### Task 6: Compilation workflow + prompt export (atomic file + DB)

**Files:**
- Create: `apps/desktop/src-tauri/src/cinema/export.rs`
- Modify: `apps/desktop/src-tauri/src/cinema/service.rs` (add `compile_and_export`)
- Test: `apps/desktop/src-tauri/tests/cinema_export.rs` + inline

**Interfaces:**
- Consumes: `compiler::compile`, `tbd_guard::check`, `repository::insert_compilation`
- Produces: `CinemaService::compile_scene(project_root, input) -> CinemaCompilation` persists DB + file

- [x] **Step 1: Write failing export test**

```rust
#[test]
fn compile_and_export_writes_deterministic_json_and_records_sha() {
  let dir = tempdir().unwrap();
  let project = create_project(&dir);
  // setup character+world+scene+shots as before
  let compilation = CinemaService::compile_scene(&dir.path().join(""), CinemaCompileInput{sceneId:..., totalDurationSeconds:8.0})?;
  assert!(dir.path().join(&compilation.exportPath).exists());
  let bytes = std::fs::read(dir.path().join(&compilation.exportPath)).unwrap();
  let sha = sha2::Sha256::digest(&bytes);
  assert_eq!(compilation.exportSha256, hex::encode(sha));
  // second compile same scene => different compilation id but same prompt content determinism if inputs same
}
```

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test cinema_export`
Expected: FAIL

- [x] **Step 2: Implement export + compile_and_export**

`export.rs`:
- `pub fn export_compilation(project_root: &Path, compilation: &ProviderNeutralCinemaPrompt) -> Result<(String, String), AppError>` — writes to `project_root/prompts/cinema/{compilationId}.json` atomically (write to .tmp then rename), returns (relative path, sha256 hex), also optionally writes `.md` human readable.

`service.rs::compile_and_export`:
- transaction: open conn, validate_scene_for_compilation, check TBD firewall, resolve behavioral locks for each character in scene, resolve world continuity, list shots, call compiler::compile, call export::export_compilation, insert into cinema_compilations with compiled json, commit.

- [x] **Step 3: Run export tests**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test cinema_export`
Expected: PASS

- [x] **Step 4: Commit**

```bash
git add apps/desktop/src-tauri/src/cinema/export.rs apps/desktop/src-tauri/src/cinema/service.rs
git commit -m "feat: add cinema compilation export (atomic JSON + sha)"
```

---

### Task 7: Tauri commands + wiring

**Files:**
- Create: `apps/desktop/src-tauri/src/cinema/commands.rs`
- Modify: `apps/desktop/src-tauri/src/cinema/mod.rs` (expose commands)
- Modify: `apps/desktop/src-tauri/src/lib.rs:21-68` (register commands)
- Modify: `apps/desktop/src-tauri/src/error.rs` (add `SceneNotFound`, `ShotNotFound`, `CinemaCompilationNotFound`, `InvalidSceneTitle`, `InvalidShotIntent`, `ProtectedTbdBlocksCompilation` if not already covered)
- Test: `apps/desktop/src-tauri/tests/cinema_commands.rs` (integration via command functions)

**Interfaces:**
- Consumes: `CinemaService`
- Produces: Tauri `create_scene`, `list_scenes`, `get_scene_with_shots`, `add_scene_character`, `add_scene_prop`, `create_shot`, `list_shots`, `compile_cinema`, `get_cinema_compilation`, `list_cinema_compilations`

- [x] **Step 1: Write failing command test**

```rust
#[test]
fn tauri_create_scene_and_compile_via_commands() {
 let dir = tempdir().unwrap(); let project = create_project(&dir);
 // call cinema::commands::create_scene(project_root_path, title, world_asset_version_id, canon_notes)
 // add character, create shots via commands
 // compile_cinema(project_root_path, scene_id, total_duration)
 // assert result contains behavioral locks and world continuity
}
```

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test cinema_commands`
Expected: FAIL — commands not found

- [x] **Step 2: Implement commands**

`commands.rs`: each `#[tauri::command] pub fn xyz(project_root_path: String, ...) -> Result<..., AppCommandError>` validates `project_service::validate_root_path`, then calls `CinemaService` with `PathBuf::from(project_root_path)` handling AppError -> AppCommandError.

Commands:
- `create_scene(project_root_path, title, worldAssetVersionId, canonNotes) -> SceneRecord`
- `list_scenes(project_root_path) -> Vec<SceneRecord>`
- `get_scene(project_root_path, scene_id) -> SceneDetail {scene, characters, props, shots}`
- `add_scene_character(project_root_path, scene_id, characterEntityId, lookAssetVersionId, sheetAssetVersionId)`
- `add_scene_prop`
- `create_shot(project_root_path, scene_id, ordering, durationSeconds, intent, action, camera)`
- `compile_cinema(project_root_path, scene_id, totalDurationSeconds) -> CinemaCompilation`
- `get_cinema_compilation`, `list_cinema_compilations`

- [x] **Step 3: Wire in lib.rs**

Add to `invoke_handler!` all new commands.

- [x] **Step 4: Run command tests**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test cinema_commands`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/cinema/commands.rs apps/desktop/src-tauri/src/cinema/mod.rs apps/desktop/src-tauri/src/lib.rs apps/desktop/src-tauri/src/error.rs
git commit -m "feat: wire cinema Tauri commands"
```

---

### Task 8: End-to-end acceptance — 8s scene with character sheet + world plate + TBD guard

**Files:**
- Create: `apps/desktop/src-tauri/tests/cinema_acceptance.rs`
- Modify: `packages/domain/src/cinema.ts` (if needed for export types)
- Test: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test cinema_acceptance`

**Interfaces:**
- Consumes: all prior cinema APIs + canon/assets helpers

- [x] **Step 1: Write failing acceptance test**

Mirrors master plan #41 acceptance:

```rust
#[test]
fn p8_acceptance_one_sheet_one_world_8s_compiles_coherently() {
 // 1. Create project
 // 2. Create character canon entity + locked speech/movement/stillness + visual_locks (scar)
 // 3. Create canonical face_lock + outfit + character_sheet assets (canonical)
 // 4. Create world_plate canonical
 // 5. Ensure no protected TBD -> compile should succeed
 // 6. Create scene linking character look=outfit version, sheet version, world plate version
 // 7. Create 2 shots (4s each, intents)
 // 8. Call compile_cinema(scene_id, 8) -> assert:
 //    - totalDuration 8, shots 2, sum durations 8
 //    - behavioralLocks contain speech/movement/stillness
 //    - worldContinuity plateId matches
 //    - providerPrompt contains time budget + behavioral + shot instructions + world continuity
 //    - no TBD topic "behind red door" in prompt when TBD protected
 // 9. Insert protected TBD "what is behind red door" -> next compile should Err WorkflowBlockedByProtectedTbd
 // 10. Resolve TBD -> compile succeeds again
}
```

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test cinema_acceptance`
Expected: FAIL until all prior tasks done

- [x] **Step 2: Fix any gaps until acceptance passes**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test cinema_acceptance -- --nocapture`
Expected: PASS after iterating on bugs (e.g., ensure visual_locks sorted, prompt template deterministic, TBD guard checks entity linkage)

- [x] **Step 3: Run full Rust + TS suite**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`
Run: `pnpm --filter @cinematic/domain test`
Run: `pnpm -r test`
Expected: PASS

- [x] **Step 4: Commit**

```bash
git add apps/desktop/src-tauri/tests/cinema_acceptance.rs
git commit -m "test: add P8 cinema acceptance (8s sheet+world+TBD)"
```

---

### Task 9: Verification + docs

**Files:**
- Modify: `docs/superpowers/plans/2026-08-28-p8-cinema-compiler.md` (check off tasks)
- Test: no code, just verification commands

- [x] **Step 1: Run idempotency check for migrations**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml db::migrations::tests::running_migrations_twice_is_idempotent`
Expected: PASS

- [x] **Step 2: Verify no provider leakage**

Grep: `ProviderNeutralCinemaPrompt` — should not contain `providerId`/`modelId`
Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml cinema::model::tests::compilation_has_no_provider_fields`
Expected: PASS

- [x] **Step 3: Build check (no Tauri build needed, just cargo check)**

Run: `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml`
Expected: PASS

