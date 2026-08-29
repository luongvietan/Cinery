# Task 6 — Implement Scene Persistence and Exact Pinning — Report

**Status:** Complete — Scene persistence with exact immutable pinning implemented, TDD verified, app runnable.

**Branch:** `feat/world-scene-pipeline`
**Workdir:** `C:\Users\admin\Desktop\Cinery\.worktrees\world-scene-pipeline`

## Commits

- `0eae278 feat: add exact scene reference pinning` — adds `scenes/repository.rs`, `scenes/service.rs`, `scenes/commands.rs`, updates `scenes/mod.rs`, `error.rs` (scene errors), `lib.rs` (Tauri command registration). Prior commits on branch: `7eca44f`, `3e6cf33`, `f2e6739`, `694c3f2`, `0968ef0`.

Commit payload:
```
6 files changed, 1691 insertions(+)
 create mode 100644 apps/desktop/src-tauri/src/scenes/commands.rs
 create mode 100644 apps/desktop/src-tauri/src/scenes/repository.rs
 create mode 100644 apps/desktop/src-tauri/src/scenes/service.rs
```

## Implementation Summary

### Error model — `apps/desktop/src-tauri/src/error.rs:216`
Added scene-specific `AppError` variants with screaming-snake codes:
- `SceneNotFound` → `SCENE_NOT_FOUND`
- `InvalidSceneTitle` → `INVALID_SCENE_TITLE`
- `SceneWorldPlateNotCanonical(String)` → `SCENE_WORLD_PLATE_NOT_CANONICAL`
- `SceneCharacterLookNotCanonical` → `SCENE_CHARACTER_LOOK_NOT_CANONICAL`
- `SceneCharacterLookNotOwned` → `SCENE_CHARACTER_LOOK_NOT_OWNED`
- `SceneCharacterAlreadyExists` → `SCENE_CHARACTER_ALREADY_EXISTS`
- `SceneCharacterSheetNotCanonical` / `SheetNotOwned`
- `ScenePropNotCanonical` / `ScenePropInvalidType` / `ScenePropAlreadyExists`

### Repository — `apps/desktop/src-tauri/src/scenes/repository.rs:1`
- `insert_scene`, `get_scene`, `list_scenes`, `next_ordinal` (`SELECT COALESCE(MAX(ordinal),0)+1`), `update_scene_details`, `update_scene_world`
- `scene_characters`: insert/list/find/delete, handles UNIQUE(scene_id,character_entity_id) mapping to `SceneCharacterAlreadyExists`
- `scene_props`: insert/list/find/delete, handles UNIQUE(scene_id,prop_asset_version_id) → `ScenePropAlreadyExists`
- `scene_reference_events`: append-only `insert_reference_event`, `list_reference_events`
- Row mappers preserve `world_id`, `world_asset_version_id` (exact version ID) per spec 3.1

### Service — `apps/desktop/src-tauri/src/scenes/service.rs:1`
All methods scope via `ProjectService::open` + `db::open_existing_connection` + `run_migrations`, validate `project_id` isolation, use ULID + `chrono::Utc::now().to_rfc3339()`:

- `create_scene(project_root, title, summary)` — validates title non-empty, immediate TX ordinal allocation, summary may be empty (readiness false). `service.rs:14`
- `list_scenes` / `get_scene` — project-isolated fetch. `:38`, `:46`
- `update_scene_details` — title re-validation, immediate TX. `:58`
- `assign_scene_world` — loads world, checks `world.project_id`, resolves `asset.canonical_version_id`, verifies `asset_version.status == canonical` and `asset_id` match, stores exact `world_asset_version_id` in TX, emits `Pin`/`Upgrade` event. Critical invariant: never stores just `asset_id`. `:84`
- `clear_scene_world` — clears both cols, emits `Remove` event. `:158`
- `add_scene_character` — validates canon entity is `character` and same project, looks up `look_version` status canonical, verifies `look_asset.owner_entity_id == characterEntityId` and type not `world_plate/prop_plate/shot_keyframe`, optionally validates `sheet_version` canonical + same owner (accepts `character_sheet`/`outfit`), checks UNIQUE, inserts assignment + `Pin` events for look/sheet. `:187`
- `remove_scene_character` — TX delete + `Remove` events. `:310`
- `add_scene_prop` — validates `prop_version.status == canonical`, `asset.type == prop_plate`, same project, UNIQUE. `:358`
- `remove_scene_prop` / lists — analogous.

Exact pinning is enforced: `scenes.world_asset_version_id` stores `asset_versions.id`, not `assets.id`. Promotion path `asset_repository::promote_canonical_version` mutates `assets.canonical_version_id` but never touches `scenes` rows.

### Commands — `apps/desktop/src-tauri/src/scenes/commands.rs:1`
Tauri IPC wrappers for frontend: `create_scene`, `list_scenes`, `get_scene`, `update_scene_details`, `assign_scene_world`, `clear_scene_world`, `add_scene_character`, `remove_scene_character`, `list_scene_characters`, `add_scene_prop`, `remove_scene_prop`, `list_scene_props`. Registered in `lib.rs:69`.

### Migrations
Reuses existing `0012_world_scene_pipeline.sql` (worlds, scenes, scene_characters, scene_props, scene_tbd_bindings, scene_reference_events). No new migration needed; schema already enforces FX keys + UNIQUE ordinal/character/prop.

## Tests — TDD

All tests live in `apps/desktop/src-tauri/src/scenes/service.rs:410` (inline `#[cfg(test)]`).

Executed via:
```
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -j1
```

**Scene creation ordinal & restart (SCENE-001/002):**
- `create_scene_allocates_ordinal_one_and_two` — asserts 1,2 sequential.
- `ordinal_persists_after_restart` — `ProjectService::open` restart, MAX ordinal still 2, direct DB check.
- `create_scene_rejects_empty_title` — `INVALID_SCENE_TITLE`, no row created.
- `create_scene_allows_empty_summary` — empty summary permitted.
- `update_scene_details_changes_title_and_summary` + `get_scene_is_project_isolated`

**World assignment:**
- `assign_scene_world_resolves_canonical_once_and_stores_exact_version` — pins V01.
- `assign_scene_world_rejects_when_no_canonical` — `SCENE_WORLD_PLATE_NOT_CANONICAL`.
- `assign_scene_world_fails_for_missing_world`

**Character assignment rejects (per spec 14):**
- `add_scene_character_rejects_non_canonical_look`
- `add_scene_character_rejects_look_owned_by_another_character`
- `add_scene_character_rejects_incompatible_sheet_wrong_owner`
- `add_scene_character_rejects_non_canonical_sheet`
- `add_scene_character_succeeds_with_valid_canonical_look` (+ uniqueness check)
- `add_scene_character_with_valid_sheet_succeeds`
- `remove_scene_character_deletes_assignment`

**Prop assignment:**
- `add_scene_prop_requires_canonical_prop_plate`
- `add_scene_prop_rejects_wrong_asset_type` (world_plate → `SCENE_PROP_INVALID_TYPE`)
- `add_scene_prop_succeeds` (+ duplicate check)
- `remove_scene_prop_deletes_assignment`

**Central pinning regression (Task 6 Step 6):**
- `world_pinning_is_immutable_after_promotion_and_survives_restart` — V01 pinned, promote V02 via `AssetService::promote_asset_version`, assert `asset.canonical_version_id == V02` but `scene.world_asset_version_id == V01`, reopen via `ProjectService::open` and direct DB reopen both still V01.
- `character_pinning_is_immutable_after_promotion` — same for Look V01 pinned, promote V02, still V01 after restart.
- `clear_scene_world_removes_pin_and_creates_event` — verifies `Remove` event with `from_version_id`.

## Verification

- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -j1 --lib` => **191 passed, 0 failed** (168 pre-existing + 23 new scene tests).
- `cargo test -j1` (full suite inc. integration tests) => all integration test binaries pass: generation_* (2+2+1+2+2+4), qa_* (3+5+3+2+2+4+4), workflow_runtime_acceptance (7), sprint_one_acceptance (1), etc. See full log in this worktree.

App remains runnable (`cargo check` passes, lib.rs builds).

## Concerns & Follow-ups

- **Sheet type strictness:** Currently accepts `character_sheet` or `outfit` owned by same character as valid sheet. If P5 relationship requires sheet linked to specific Look via `asset_relationships`, that table does not exist in current schema; enforcement would need a follow-up migration/query. Current minimum-owner check satisfies spec’s “at least same owner” fallback.
- **Look asset type:** Rejects `world_plate/prop_plate/shot_keyframe` for Look but otherwise permissive (allows `face_lock`/`outfit`/`image`). If product requires `outfit` only, tighten to `asset_type == "outfit"` post-P5 clarification. Existing QA context treats `character_sheet`/`outfit` as look reference.
- **World assignment re-pin:** `assign_scene_world` emits `Upgrade` if previous pin differs, `Pin` if first; explicit upgrade operations per spec 21 (`upgrade_scene_world_reference` etc.) are Task 7 scope — current pin path covers initial assignment correctly.
- **TBD bindings:** Table exists but service helpers not exposed yet (Task 6 note: persistence not broken). Task 7+ will add `set_scene_tbd_binding` helpers.
- **Readiness/Health:** Derived resolvers (`resolve_scene_references`, `SceneReadiness`) are Task 7 scope, not included here to keep Task 6 minimal.
- **Project isolation:** Verified for scenes; asset project isolation also checked.

## Report Path

`C:\Users\admin\Desktop\Cinery\.worktrees\world-scene-pipeline\.superpowers\sdd\2026-08-28-world-scene-pipeline\task-6-report.md`
