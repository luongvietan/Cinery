# P9 Task 2 review — commit `e975396`

## Verdict

- **Spec compliance: FAIL**
- **Code quality: FAIL**

The deterministic canonical-asset stages and all 11 focused TDD fixtures pass,
and the change does not add a migration or persist duplicate readiness state.
However, the overview is not read-only, the current TBD firewall can be masked
by an older compilation, and several primary actions route to workspaces that
cannot perform the recommended operation. Those are core acceptance failures,
not polish issues.

## Findings

### [P1] Reading the overview can mutate authoritative workflow state

- **File:** `apps/desktop/src-tauri/src/integration/readiness.rs:81`
- **Related owner:** `apps/desktop/src-tauri/src/project/service.rs:71`

`get_project_overview` calls `ProjectService::open`. That method runs migrations
and then calls `recover_interrupted_runs`, which updates running workflow steps
and runs to `failed` and appends durable events. Therefore a command documented
as a "derived, read-only project production overview" can change business state
merely because the UI rendered it. This violates Task 2's read-only projection
semantics and the frozen rule that existing services/database remain the sole
mutable authority. Use a non-recovering identity/read connection for this query,
or make project recovery an explicit lifecycle action outside the overview.

### [P1] A protected TBD opened after compilation is incorrectly reported complete

- **File:** `apps/desktop/src-tauri/src/integration/readiness.rs:218`

`derive_readiness` returns `Complete` as soon as any compilation exists, before
checking `has_blocking_scene_tbd` at line 230. If a protected project/cast TBD is
opened or reopened after compilation, the current scene has a blocking TBD but
the overview still says "Production path complete." That contradicts the TDD
requirement "Scene with blocking TBD reports blocked rather than ready" and the
frozen current-state TBD firewall. Check the blocker before returning complete
(or explicitly model a stale compilation) and add the compile-then-open/reopen
regression case.

### [P1] Core next actions route to dead ends instead of the required operation

- **File:** `apps/desktop/src-tauri/src/integration/readiness.rs:129`
- **File:** `apps/desktop/src-tauri/src/integration/readiness.rs:212`
- **File:** `apps/desktop/src-tauri/src/integration/readiness.rs:245`
- **Related UI:** `apps/desktop/src/features/production/ProductionWorkspace.tsx:21`

The missing-face action routes to `production`, but that workspace derives its
only target from an already-canonical `face_lock` and disables "Create Face
Lock" when none exists. The exact state that produces the action therefore
cannot be resolved at its destination. The Scene and Cinema Compilation actions
also route to `production`, while that workspace contains only the character
face-lock flow; there is no scene/shot/compile UI or frontend cinema adapter in
the diff or existing feature tree. The button changes tabs but cannot execute
the recommended next step, so the "actionable overview" and acceptance goal are
not met. Route each action to a workspace that can perform it, or add the missing
scene/cinema flows and destination type.

### [P2] The readiness cards discard their own action metadata

- **File:** `apps/desktop/src/features/overview/ProjectOverview.tsx:38`
- **File:** `apps/desktop/src/features/overview/ProjectOverview.tsx:45`

The backend and shared type attach `action` to each `ReadinessStep`, but React
only renders one header-level `nextAction` button. Each card is a non-interactive
`li`; `step.action` is never read. This does not implement the brief's explicit
"Overview should use actionable cards" requirement and also leaves no scoped
action affordance if the overview later contains multiple blocked/pending items.

### [P2] Canonical readiness performs an avoidable per-character query loop

- **File:** `apps/desktop/src-tauri/src/integration/readiness.rs:118`
- **File:** `apps/desktop/src-tauri/src/integration/readiness.rs:313`

For a project with `N` characters, readiness issues up to `3N` separate
`SELECT EXISTS` calls for face, outfit, and sheet, in addition to the other
overview queries and the extra project-open connections. This is an N+1 pattern
on the default project landing screen. One grouped query over characters/assets
can derive all three stage counts deterministically in constant round trips.

### [P2] Selecting only the oldest eligible scene can hide later project work

- **File:** `apps/desktop/src-tauri/src/integration/readiness.rs:326`

`valid_scene_id` orders ascending and applies `LIMIT 1`; all blocker and compile
decisions are then made for only that scene. In a multi-scene project, an old
compiled scene can make the whole production path complete while a newer scene
is uncompiled or blocked. A project-level overview should aggregate/surface each
scene or use an authoritative current-scene selection rather than silently
equating the oldest eligible scene with the entire project.

## Requirement verification

| Requirement | Result | Evidence |
| --- | --- | --- |
| Empty project -> Story Canon | PASS | Focused Rust test passes. |
| Character without canonical Face -> Face Lock | PARTIAL | Derivation/test pass; action destination is self-blocking. |
| Newest Face candidate is not canonical | PASS | Query follows `assets.canonical_version_id` and requires version status `canonical`; focused test passes. |
| Canonical Face -> Look -> Sheet -> World | PASS | Focused tests pass; no newest-version inference. |
| World + Character -> Scene | PARTIAL | Derivation passes; Scene action has no capable UI target. |
| Blocking TBD -> blocked | FAIL | Basic pre-compile fixture passes, but an existing compilation bypasses the current blocker. |
| Valid Scene -> Cinema compilation | PARTIAL | Derivation passes; action has no capable UI target. |
| Completed compilation -> complete | PASS for the single-scene/no-new-blocker fixture | Focused Rust test passes. |
| Superseded exact refs remain valid/not missing | PASS | Scene query checks persisted exact IDs without re-resolving to newest/current pointers; focused test passes. |
| Backend authority/no duplicate mutable readiness | PASS | Rust derives the shape from SQLite; React stores display state only; no migration/new store. |
| Read-only semantics | FAIL | `ProjectService::open` can durably recover/fail workflow runs. |
| Shared type boundary | PASS | Rust public fields are camelCase, enums snake_case, and the TS transport shape matches. |
| Loading/error states | PASS in implementation, untested | Component renders `role=status` while loading and `role=alert` on rejection; the two UI tests do not exercise either path. |
| Query efficiency | FAIL | Up to `3N` canonical-asset queries plus fixed overview/open queries. |

## Verification evidence

- Inspected the complete commit range: `git diff 8bd6196 e975396` (13 files,
  including the submitted report).
- Used Graft to trace `get_project_overview` and its downstream calls. The trace
  exposed `ProjectService::open -> recover_interrupted_runs`; the blast-radius
  report also showed no test reaches the action constructors or workspace
  navigation behavior.
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test project_overview`
  — **11 passed, 0 failed**.
- `pnpm --filter @cinematic/desktop test -- ProjectOverview.test.tsx`
  — **2 passed, 0 failed**.
- `pnpm --filter @cinematic/desktop build` — **passed**.
- Pre-existing dirty QA files and `ProjectWorkspace.test.tsx` were treated as
  outside scope and were not used as findings.

## Code-quality notes that passed

- Canonical checks use the authoritative pointer plus exact canonical status;
  newest candidates are not inferred.
- Persisted scene references are not rewritten or refreshed to newer versions.
- The frontend API is a thin Tauri adapter, and no provider-specific or mutable
  readiness authority was introduced.
- The component guards against stale async responses when the project path
  changes and exposes basic loading/error roles.

**Review result: CHANGES REQUIRED.**

## Round 1 corrections

- Replaced `ProjectService::open` in the overview path with a manifest +
  existing-connection identity check. The derived query neither migrates nor
  invokes interrupted-workflow recovery.
- Added a grouped character/assets query that derives face/look/sheet
  canonicality in one round trip, retaining the exact canonical pointer and
  canonical-version-status rules.
- Added deterministic per-scene readiness (newest first). Any current protected
  TBD for a valid scene takes precedence over previous compilation; pending and
  blocked scene actions retain the exact `sceneId` metadata.
- Extended every overview action with scoped character/scene metadata. Header,
  production-step, and scene cards all consume the action rather than reducing
  it to a destination string.
- Added the Scene & Cinema workspace. It stages a scene through the existing
  scene/cast/shot commands using canonical records, then compiles the selected
  persisted scene through the existing P8 command. Asset actions now carry the
  character owner to the existing create/import/promote asset flow; protected
  TBD actions open the Canon TBD tab.

## Round 1 TDD evidence

- RED: the added Rust multi-scene regression initially failed to compile because
  `OverviewAction.sceneId` and `ProjectOverview.sceneReadiness` did not exist.
  The card-scope regressions then failed on missing character/scene metadata.
- GREEN: `project_overview` now passes 14 tests, including no-recovery reads,
  post-compilation TBD blocking, scoped card actions, and older-compiled versus
  newer-uncompiled scene ordering.
- RED: the added overview-card and CinemaWorkspace tests failed because cards
  had no actionable controls and the cinema workspace did not exist.
- GREEN: overview/CinemaWorkspace tests pass (6 tests), including actual staged
  scene command sequencing and scoped compile duration submission.

## Round 1 verification

- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test
  project_overview` — passed (14).
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test
  cinema_compiler` — passed (2).
- `pnpm --filter @cinematic/domain test` — passed (26).
- `pnpm --filter @cinematic/desktop test` — passed (44).
- `pnpm --filter @cinematic/desktop build` — passed.

The full Rust suite remains blocked by the unchanged dirty QA integration test
work described above; no QA or graft cache file was modified or staged.
