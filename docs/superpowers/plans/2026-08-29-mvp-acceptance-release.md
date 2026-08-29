# MVP Acceptance and Release Qualification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Prove the complete deterministic MVP through public Tauri command boundaries and a stateful desktop UI test, then build the production bundle and record honest release-candidate and clean-install evidence.

**Architecture:** Add one Rust acceptance fixture that owns real project storage/SQLite but invokes only public Tauri command functions and DTOs for mutations. Replace the static UI golden test with a stateful command facade whose responses evolve across the MVP flow. A reproducible release script runs installs, tests, build, bundle, integrity checks, and writes machine-readable evidence; the `MVP IMPLEMENTED` label is gated behind a separate clean-install manual record.

**Tech Stack:** Rust integration tests, Tauri 2 commands, tempfile, deterministic mock providers/QA; React, TypeScript, Vitest, React Testing Library; PowerShell release script, pnpm, Cargo, Tauri bundler.

**Spec:** `docs/superpowers/specs/2026-08-29-mvp-acceptance-release-design.md`

## Global Constraints

- The acceptance test must mutate state only through public command functions/DTOs and assert `AppCommandError` at that boundary.
- Service/repository helpers may construct the test harness but must not perform acceptance-flow mutations.
- Default tests use deterministic local mock providers and QA adapters; no external credentials or network.
- Do not claim a clean-install pass without a real installation on a clean user profile or machine and recorded artifact evidence.
- Until that manual evidence exists, documentation must say `MVP RELEASE CANDIDATE`, not `MVP IMPLEMENTED`.
- Production bundle output must be identified by absolute path and SHA-256.
- Preserve the dirty working tree and stage only agent-owned hunks.

---

### Task 1: Create a command-boundary MVP acceptance harness

**Files:**
- Create: `apps/desktop/src-tauri/tests/support/command_harness.rs`
- Modify: `apps/desktop/src-tauri/tests/support/mod.rs`
- Create: `apps/desktop/src-tauri/tests/mvp_command_acceptance.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/src/project/commands.rs`
- Modify: `apps/desktop/src-tauri/src/canon/commands.rs`
- Modify: `apps/desktop/src-tauri/src/generation/commands.rs`
- Modify: `apps/desktop/src-tauri/src/qa/commands.rs`
- Modify: `apps/desktop/src-tauri/src/workflow/commands.rs`
- Modify: `apps/desktop/src-tauri/src/cinema/commands.rs`
- Modify: `apps/desktop/src-tauri/src/integration/commands.rs`

**Harness produced:**

```rust
struct CommandHarness {
    temp: TempDir,
    app: TestAppState,
}

impl CommandHarness {
    fn invoke<T>(&self, call: impl FnOnce(&TestAppState) -> Result<T, AppCommandError>) -> T;
    fn reopen(self) -> Self;
}
```

- [x] **Step 1: Write the first failing command-only project/canon test**

Create a project, reopen it, write required Canon sections, and inspect overview/readiness using command functions. Add a test guard/comment convention that acceptance mutations may not call `*Service` or `*Repository` methods.

- [x] **Step 2: Run RED**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test mvp_command_acceptance -- --nocapture`

Expected: missing harness/public command access or incomplete setup.

- [x] **Step 3: Implement the smallest injectable Tauri state wrapper**

Reuse production repositories, migrations, paths, command DTOs, and error conversion. Inject deterministic provider/QA adapters at state construction only.

- [x] **Step 4: Run GREEN for project/canon setup**

Suggested commit: `test: add Tauri command acceptance harness`

### Task 2: Drive the complete character pipeline through commands

**Files:**
- Modify: `apps/desktop/src-tauri/tests/mvp_command_acceptance.rs`
- Modify: `apps/desktop/src-tauri/tests/support/command_harness.rs`

- [x] **Step 1: Add failing Face Lock command sequence**

Through command DTOs: create run, compile, approve, execute, list result sets, run QA, record a deterministic failure, create repair, execute repair, record pass, promote candidate, and assert canonical Face version.

- [x] **Step 2: Run RED and make only harness/public-boundary fixes**

Do not bypass missing commands with direct services. If a necessary command is absent, add it with its own focused RED/GREEN command test first.

- [x] **Step 3: Add failing Outfit sequence**

Create Outfit using the canonical Face reference; assert compiled input version, provider/model, result gallery persistence, QA result, explicit promotion, and canonical Outfit version.

- [x] **Step 4: Add failing Character Sheet sequence**

Create Sheet using pinned Face + Outfit references; assert exact version IDs, result persistence, and explicit canonical promotion.

- [x] **Step 5: Run GREEN**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test mvp_command_acceptance character_pipeline -- --nocapture`

Suggested commit: `test: cover character MVP through Tauri commands`

### Task 3: Drive world, prop, keyframe, scene, compile, provenance, and reopen

**Files:**
- Modify: `apps/desktop/src-tauri/tests/mvp_command_acceptance.rs`

- [x] **Step 1: Add failing canonical reference setup**

Import World, Prop, and Keyframe assets through asset commands, create versions, and promote exact versions through public generation/asset commands.

- [x] **Step 2: Add failing Cinema sequence**

Create/rename Scene, set World, add Character with Face/Outfit/Sheet pins, add Prop, create/reorder/update Shot, set Keyframe, inspect structured readiness, and compile.

- [x] **Step 3: Add failing provenance and reopen assertions**

Assert compile ID/path/hash/time, every pinned input version, lineage/provenance traversal, exported payload, then close/reopen the project and assert the same canonical selections, result sets, scene graph, and compilation record.

- [x] **Step 4: Run RED, implement only missing public behavior, then GREEN**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test mvp_command_acceptance -- --nocapture`

Suggested commit: `test: cover cinema compile and reopen through commands`

### Task 4: Replace the static UI golden test with a stateful command facade

**Files:**
- Rewrite: `apps/desktop/src/__tests__/mvp-golden-path.test.tsx`
- Create: `apps/desktop/src/test/StatefulTauriFacade.ts`
- Modify: `apps/desktop/src/test/setup.ts`
- Modify: feature tests only if shared fixtures need adoption

**Facade produced:**

```ts
export class StatefulTauriFacade {
  private state: DesktopFixtureState;
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  snapshot(): Readonly<DesktopFixtureState>;
}
```

- [x] **Step 1: Write a failing facade state-transition test**

Assert create/update/promote/compile calls mutate subsequent list/detail responses, invalid calls reject with the same normalized shape as `AppCommandError`, and reopening a project retains fixture state.

- [x] **Step 2: Run RED**

Run: `pnpm --filter @cinematic/desktop test -- mvp-golden-path.test.tsx`

- [x] **Step 3: Implement only commands exercised by the MVP path**

Use exhaustive command matching that throws on unknown commands; never return one static canned response for all states.

- [x] **Step 4: Drive the visible desktop flow**

The test must interact like a user: open project, finish Canon, run Face with QA fail/repair/promote, run Outfit/promote, run Sheet/promote, assemble Cinema references, compile, inspect provenance, navigate away/back, and verify persisted results.

- [x] **Step 5: Assert meaningful UI state at each boundary**

Prefer roles/labels/text visible to users. Assert disabled/blocker states, progress, errors, selected provider/model, gallery candidates, explicit promotion, compile evidence, and reload.

- [x] **Step 6: Run GREEN and commit**

Run: `pnpm --filter @cinematic/desktop test -- mvp-golden-path.test.tsx`

Suggested commit: `test: exercise stateful desktop MVP golden path`

### Task 5: Add reproducible release qualification and evidence generation

**Files:**
- Create: `scripts/verify-mvp-release.ps1`
- Create: `docs/release-evidence/README.md`
- Modify: `package.json`
- Modify: `.gitignore` only if generated evidence needs exclusions
- Test: invoke the script with its non-bundle test mode first

**Script contract:**

```powershell
param(
  [switch]$SkipInstall,
  [switch]$SkipBundle,
  [string]$EvidenceDate = (Get-Date -Format 'yyyy-MM-dd')
)
```

The script stops on first failure, records command, exit code, tool versions, Git commit/dirty state, artifact paths, sizes, hashes, and timestamps. It never writes `cleanInstallPassed: true`.

- [x] **Step 1: Write a failing Pester-free smoke invocation**

Run the script with `-SkipInstall -SkipBundle` and assert it refuses to write a passing record if any required command fails. Keep it runnable with stock PowerShell.

- [x] **Step 2: Implement ordered gates**

```powershell
pnpm install --frozen-lockfile
pnpm test
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -j 1
pnpm --filter @cinematic/desktop build
pnpm --filter @cinematic/desktop tauri build
git diff --check
```

Resolve bundle artifacts only beneath `apps/desktop/src-tauri/target/release/bundle`; validate each resolved absolute path stays under that directory before hashing.

- [x] **Step 3: Emit release-candidate evidence**

Write `docs/release-evidence/<date>-mvp-release-candidate.md` with a table of gates and a clearly unchecked clean-install section.

- [x] **Step 4: Run the non-bundle script path GREEN**

Run: `powershell -ExecutionPolicy Bypass -File scripts/verify-mvp-release.ps1 -SkipInstall -SkipBundle`

Suggested commit: `build: add reproducible MVP release verification`

### Task 6: Correct release labels and clean-install protocol

**Files:**
- Modify: `README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/release-checklist.md`
- Create: `docs/release-evidence/clean-install-template.md`

- [x] **Step 1: Add a failing documentation assertion**

Use an `rg`/PowerShell check that fails while unqualified `MVP IMPLEMENTED` appears outside historical evidence and requires `MVP RELEASE CANDIDATE` plus the clean-install gate.

- [x] **Step 2: Update status language**

Document states exactly:

```text
MVP DEVELOPMENT
  -> automated gates + production bundle pass
MVP RELEASE CANDIDATE
  -> clean-profile install/launch/full-flow/uninstall evidence pass
MVP IMPLEMENTED
```

The README must describe OS-keychain configuration, not environment variables as the normal mechanism.

- [x] **Step 3: Define the manual clean-install record**

Require OS/build, installer path/hash, clean account/profile, install result, first launch, project creation, provider configuration, deterministic/full smoke flow, reopen, export, uninstall, residual-data observation, screenshots/log paths, tester, timestamp, and pass/fail.

- [x] **Step 4: Run documentation check and commit**

Suggested commit: `docs: gate MVP implemented status on clean install`

### Task 7: Run production bundle and record release-candidate evidence

**Files:**
- Generate: `docs/release-evidence/<date>-mvp-release-candidate.md`
- Do not edit the clean-install result unless the manual test was truly performed

- [x] **Step 1: Capture the exact baseline**

Run: `git status --short`

Record HEAD, dirty paths, Node, pnpm, Rust, Cargo, and Tauri versions in evidence.

- [x] **Step 2: Run the complete automated gate including bundle**

Run: `powershell -ExecutionPolicy Bypass -File scripts/verify-mvp-release.ps1`

Expected: all tests/builds pass and at least one signed/unsigned platform installer artifact is found and hashed.

- [x] **Step 3: Inspect bundle output**

Verify file existence, non-zero size, SHA-256, expected app identifier/name/version, and that no raw credential sentinel appears in unpacked readable resources.

- [x] **Step 4: Record honest status**

If automated gates pass, label the build `MVP RELEASE CANDIDATE`. Keep `MVP IMPLEMENTED` blocked until Task 8 passes.

- [x] **Step 5: Commit the release-candidate evidence if it contains no secrets or machine-private paths**

Suggested commit: `chore: record MVP release candidate evidence`

### Task 8: Perform the real clean-install smoke test before promotion

**Files:**
- Create from template: `docs/release-evidence/<date>-clean-install.md`
- Modify release labels only after a real pass

- [ ] **Step 1: Move the installer to a clean machine or clean OS user profile**

The source checkout, development database, build tools, environment credential variables, and prior app data must not be available to the test user.

- [ ] **Step 2: Install and launch from the packaged application**

Record installer hash, install result, first-launch result, app version, and any security prompts.

- [ ] **Step 3: Execute the MVP smoke flow**

Create/open project; configure provider via OS keychain; complete Canon; generate/review/promote character assets; assemble World/Scene/Shot/Prop/Keyframe; compile/export; inspect provenance; quit/relaunch; verify persistence.

- [ ] **Step 4: Uninstall and inspect residual data**

Record expected retained project data versus unexpected application residue. Do not delete user project data as part of uninstall verification.

- [ ] **Step 5: Promote or reject**

Only when every clean-install item passes may README/architecture/checklist change from `MVP RELEASE CANDIDATE` to `MVP IMPLEMENTED`. Otherwise record the failure and keep the candidate label.

- [x] **Step 6: Run final integrity check**

Run: `git diff --check`

Review evidence for secrets, personal directories, tokens, or raw base64 before committing.
