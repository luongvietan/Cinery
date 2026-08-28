# World, Scene, Shot, Prop, and Keyframe Workspace Design

## Purpose

Turn the existing Cinema backend into a complete exact-reference assembly UI. Users must be able to build and revise a scene, inspect its pinned production truth, resolve readiness blockers, and compile a provider-neutral cinema prompt.

## Current State

- SQLite and Rust services support scenes, scene characters, scene props, shots, exact world/look/sheet references, and optional shot-keyframe IDs.
- The current UI automatically chooses the first canonical world/look/sheet, stages one character and one default four-second shot, then only exposes Compile.
- Backend update, removal, reorder, and keyframe assignment operations are incomplete or not exposed through commands.

## Domain Rules

- A Scene may pin one canonical World Plate version.
- A Scene may cast multiple Character entities. Each cast record pins an exact canonical Outfit version and may pin an exact canonical Character Sheet version.
- A Scene may pin multiple canonical Prop Plate versions.
- A Shot belongs to one Scene and records ordering, duration, intent, optional action, optional camera instruction, and optional canonical Shot Keyframe version.
- Promoting a newer AssetVersion never mutates any existing Scene or Shot reference.
- A stale pinned reference remains visible and blocks compilation until the user explicitly restages or selects another canonical version.
- All mutations validate project ownership and asset type in the backend.

## Backend Surface

Add command/service/repository operations for:

- creating and renaming a Scene;
- setting or clearing the Scene World Plate;
- adding, updating, and removing a cast character;
- adding and removing a Scene Prop;
- creating, updating, deleting, and reordering Shots transactionally;
- setting or clearing a Shot Keyframe;
- reading structured Scene readiness without using an exception as the normal UI state.

Delete operations affect only relationship rows or Shot records. They never delete Canon entities, Assets, AssetVersions, or media files.

Shot ordering is contiguous and deterministic after create, delete, or move. Reordering executes in one transaction and rejects duplicate or foreign Shot IDs.

## UI Information Architecture

Cinema uses three coordinated regions:

1. **Scene list:** title, world status, cast count, shot count, duration, compile status, and stale-reference indicator.
2. **Scene workspace:** World, Cast, Props, and ordered Shots. Users edit these inline with standard selects, fields, and explicit remove actions.
3. **Reference inspector:** selected exact AssetVersion, canonical/stale status, owner, preview, provenance entry point, and replacement control.

The layout collapses to a single ordered column on narrow windows while preserving the same tab order. No action is hover-only.

### World

The World section lists canonical World Plate assets and their exact versions. It links to Assets for creating/importing a World Plate when none exists. Selecting a world updates only the Scene relationship.

### Cast

The cast editor selects a Character first, then shows compatible canonical Outfit and Character Sheet versions owned by that character. Duplicate casting of the same character is rejected unless the domain later introduces explicit duplicate roles.

### Props

The Props section selects canonical `prop_plate` versions, shows previews, and permits explicit removal. The same version cannot be attached twice.

### Shots and keyframes

Each Shot row exposes duration, intent, action, camera, move up/down, delete, and keyframe selection. Keyframe choices are canonical `shot_keyframe` versions. Clearing a keyframe leaves the Shot valid but visibly incomplete if a production rule requires one.

The editor saves deliberate field changes and shows pending/error state per affected row so one failed Shot update does not freeze the entire workspace.

### Readiness and compilation

The backend returns structured blockers such as missing world, empty cast, missing shots, noncanonical reference, stale reference, invalid runtime, or protected unresolved Canon question. The UI lists blockers next to the relevant section and disables Compile until none remain.

Compile uses the exact persisted Scene detail and total Shot duration. On success, the UI shows compilation ID, export path, hash, created time, and an Open exported prompt action.

## Error and Recovery Rules

- Failed relationship mutations leave prior state unchanged.
- Concurrent stale edits reload the authoritative Scene detail and explain the conflict.
- Deleting the last Shot is allowed but makes the Scene not ready.
- Asset promotion elsewhere marks pinned older versions stale without rewriting them.
- Compile never resolves Canon questions or substitutes a newer asset automatically.

## Test Strategy

- Repository tests for relationship deletion, Shot updates, contiguous reorder, and foreign-key protection.
- Service tests for ownership/type/canonical validation and transaction rollback.
- Command tests for every new IPC payload and stable error code.
- React tests for empty Scene, complete assembly, stale references, cast/prop edits, Shot editing/reordering, keyframe assignment, blockers, and compile export.
- The command-boundary MVP acceptance test assembles a World, Prop, Scene, Shot, and Keyframe using these public commands.

## Acceptance Criteria

1. A user can create and edit a Scene without relying on automatically selected first assets.
2. A user can manage cast, Props, Shots, and Shot Keyframes through the desktop UI.
3. Every relationship pins an exact validated AssetVersion and survives restart.
4. Promoting a later World, Look, Prop, or Keyframe does not rewrite the Scene.
5. Readiness blockers identify the affected section and compilation succeeds only from valid persisted state.
6. The exported cinema prompt remains provider-neutral, deterministic, and inspectable.

## Non-Goals

- Timeline video editing or compositing.
- Storyboard canvas, batch generation, or direct video editing.
- Automatic restaging when Canon changes.
- Deleting source Assets from the Cinema workspace.
