# MVP Command Acceptance and Release Gate Design

## Purpose

Replace the current optimistic MVP claim with reproducible evidence. Add one complete acceptance chain through public Tauri command boundaries, produce a production bundle, record smoke-test evidence, and keep documentation at release-candidate status until a clean-install pass is completed.

## Current State

- Backend acceptance tests are broad, but the golden path calls service APIs and directly creates canonical Face, Outfit, Sheet, and World assets.
- The UI golden-path test mocks every data API and primarily verifies navigation.
- TypeScript tests, Rust tests, and the frontend build pass.
- Production bundle and clean-machine installation have not been recorded.
- The working tree contains post-MVP work while README states `MVP IMPLEMENTED`.

## Command-Boundary Acceptance Chain

Add a Rust integration test that imports and invokes the same public command functions registered in the Tauri application. Inputs and outputs use command DTOs and `AppCommandError`; test setup may create deterministic source images but must not call feature services to advance product state.

The scenario performs:

1. Create a project and reopen it through project commands.
2. Create Story and Character Canon, save required sections, add permanent visual locks, and lock authoritative sections.
3. Launch Face Lock with mock provider, approve, execute, choose a generated artifact, create a Face asset, and promote the new version.
4. Run visual QA with deterministic failures, review the result, execute Repair, verify a child version, rerun QA, and promote the repaired Face.
5. Launch Outfit from the canonical Face, promote its generated version, run QA, and keep the canonical Outfit.
6. Launch Character Sheet from the canonical Outfit and promote the generated version.
7. Create/import and promote World Plate, Prop Plate, and Shot Keyframe assets.
8. Create a Scene, choose World, cast the character with exact Look/Sheet versions, attach the Prop, create and edit a Shot, and attach the Keyframe.
9. Compile the Cinema prompt and verify runtime, behavioral locks, world continuity, exact references, export file, and hash.
10. Traverse provenance from the compilation back to Scene, generated assets, workflows, providers, and Canon snapshot.
11. Close and reopen the project, then verify all exact references, statuses, histories, QA decisions, and export records remain intact.

The test uses mock generation and QA adapters only. It must fail if any step bypasses approval, promotion, command validation, or exact-version persistence.

## Supporting UI Acceptance

Replace the static UI golden-path test with a stateful command-facade fixture. It exercises navigation and the user-visible transitions for operation launch, approval, result selection, promotion, Scene editing, blocker resolution, and compilation. Lower-level component tests remain focused and may mock their immediate API boundary.

A real desktop WebDriver suite is not required for this release gate because it would add OS driver dependencies. The command acceptance plus stateful UI acceptance and manual installed-app smoke pass form the release evidence.

## Automated Release Verification

The release verification sequence is:

```text
pnpm install --frozen-lockfile
pnpm test
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -j 1
pnpm --filter @cinematic/desktop build
pnpm --filter @cinematic/desktop tauri build
git diff --check
```

The bundle step must produce the configured Windows installer and executable. Verification records exact artifact paths, sizes, SHA-256 hashes, command exit codes, and application version in `docs/release-evidence/<date>-mvp-release-candidate.md`.

## Smoke Test

### Local production-binary smoke

On the development machine:

- launch the built production executable;
- create a temporary project through the UI;
- close and reopen it;
- execute the deterministic mock Face workflow and inspect its result;
- export diagnostics and verify the bundle contains no media or secrets;
- exit cleanly without modifying unrelated user projects.

### Clean-install smoke

On a Windows machine or VM without Node, pnpm, Rust, or repository files:

- install the generated bundle;
- launch from the installed shortcut;
- complete the release checklist's project, provider, mock workflow, persistence, and diagnostics steps;
- uninstall the application;
- verify user-selected project directories remain intact.

This manual pass records tester, OS version, installer hash, timestamps, result, and any deviations in the release-evidence document.

## Documentation State Machine

- Before automated verification: `MVP IN DEVELOPMENT`.
- Automated tests and production bundle pass, clean-install pending: `MVP RELEASE CANDIDATE`.
- Clean-install smoke passes with recorded evidence: `MVP IMPLEMENTED`.

README, architecture documentation, and release checklist must use the same state. Post-MVP stubs are labeled as experimental and cannot be used as evidence for MVP completion.

## Failure Rules

- A failed automated command stops release verification and records the failing command.
- Missing installer, startup failure, leaked secret, phantom asset, or persistence loss blocks the release.
- A development-machine launch is not evidence of a clean installation.
- The assistant must not mark the manual clean-machine checklist complete without an actual observed run.

## Acceptance Criteria

1. The full canonical production journey passes through public command boundaries without direct service shortcuts.
2. The stateful UI acceptance covers the same major user transitions.
3. All automated suites and the production Tauri bundle pass from the current checkout.
4. Release evidence contains artifact hashes and command results.
5. Documentation remains `MVP RELEASE CANDIDATE` until clean-install evidence is recorded.

## Non-Goals

- Code signing, notarization, auto-update, or public distribution.
- Paid-provider calls in automated tests.
- Treating a development environment as a clean-machine test.
- Shipping Post-MVP video features as part of MVP acceptance.
