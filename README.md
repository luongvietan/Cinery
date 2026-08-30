# AI Cinematic Production OS

A local-first desktop app (Tauri 2 + React + TypeScript) for managing the
project kernel and versioned media assets of an AI-assisted film production.
Every project lives entirely on disk as a directory containing a SQLite
database and managed media files -- there is no server and no cloud
dependency.

## Prerequisites

- [Node.js](https://nodejs.org/) 18+ and [pnpm](https://pnpm.io/) 9
- [Rust](https://www.rust-lang.org/tools/install) (stable toolchain)
- On Windows: [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
  with the "Desktop development with C++" workload (provides the MSVC
  linker Tauri's Rust build needs)

## Getting started

```bash
pnpm install
pnpm dev
```

`pnpm dev` launches the Tauri desktop app with hot-reloading.

## Testing and building

```bash
pnpm test              # TypeScript/React unit and component tests (Vitest)
pnpm test:rust         # Rust unit and integration tests (cargo test)
pnpm --filter @cinematic/desktop tauri build --debug   # Debug desktop build
```

## Sprint 1 scope

Sprint 1 delivers the project kernel and asset versioning foundation:
creating and reopening local projects, creating assets, importing image
versions, and promoting a version to canonical with transactional
superseding of the prior canonical version.

A manual desktop walkthrough that exercises this end-to-end lives at
[`docs/superpowers/plans/sprint-1-verification.md`](docs/superpowers/plans/sprint-1-verification.md).
An automated Rust acceptance test proving the same state machine, including
restart persistence, lives at
[`apps/desktop/src-tauri/tests/sprint_one_acceptance.rs`](apps/desktop/src-tauri/tests/sprint_one_acceptance.rs).

## Canon Engine (P2)

Canon is structured, typed data stored in each project’s SQLite database.
Locked sections are canonical; draft sections are working state. The Story
Bible Markdown file is a deterministic export, never the machine source of
truth. Protected open TBDs are explicit future-workflow firewall entries.

The Canon Engine includes section-level locking and append-only revision
history for Story, Character, Location, Faction, World Rule, and Production
Rules entities, plus restart-safe export and query boundaries for locked
visual locks, world rules, production rules, and protected TBDs.

The manual walkthrough is documented at
[`docs/superpowers/plans/canon-engine-verification.md`](docs/superpowers/plans/canon-engine-verification.md).
The automated acceptance test lives at
[`apps/desktop/src-tauri/tests/canon_engine_acceptance.rs`](apps/desktop/src-tauri/tests/canon_engine_acceptance.rs).

## MVP RELEASE CANDIDATE (P0–P9)

The current release covers the full MVP chain described in
[`docs/specs/ai-cinematic-production-os-master-plan.md`](docs/specs/ai-cinematic-production-os-master-plan.md):

- **Project kernel & asset versioning (P0–P1):** local projects, exact
  version history, transactional canonical promotion, thumbnails.
- **Canon engine (P2):** typed entities, section locking, append-only
  revisions, protected TBD firewall, deterministic Story Bible export.
- **Skill workflow runtime (P3–P4):** builtin skills (Face Lock, Visual QA),
  immutable context snapshots, approvals, provider execution with
  idempotent attempts, artifact lineage.
- **Provider integrations (P5):** mock + OpenAI adapters, capability
  disclosure, keychain-only credentials, cancel/retry UX.
- **Character pipeline (P5, §21):** the full Face Lock → Outfit → Character
  Sheet chain with enforced prerequisites (no outfit without a canonical
  face; no sheet without a canonical outfit) and three-panel sheet
  compilation (§24).
- **Visual QA & repair (P6–P7):** check planner, per-check review,
  repair-to-child-version workflow, exact-version scene pinning.
- **Cinema compiler (P8):** provider-neutral scene compilation, runtime
  budget guards, protected-TBD firewall, durable prompt export.
- **Integration & polish (P9):** project overview & readiness, health
  scanning, provenance traversal, unified job lifecycle/recovery,
  privacy hardening, diagnostics export, UX/accessibility polish,
  golden-path acceptance fixture.
- **Production router (§13):** deterministic intent → operation routing
  with code-validated prerequisites, optional LLM classification via the
  project's configured Text (LLM) service behind a hard code-validation
  boundary, surfaced by the Overview AI Director bar.
- **Video foundation (P10.0):** `scene.generate_video` animates a persisted
  scene compilation into a real video through any configured video-capable
  AI service (declarative `video.generate` / `video.imageToVideo`
  operations; Alibaba-Wan preset shipped). The output persists as a
  `video/mp4` GeneratedArtifact with full lineage, is reviewed in the
  candidate gallery, promoted explicitly into the scene's stable video
  asset, and can be pinned as a Shot's **exact** immutable video version
  (`set_shot_video`) that never drifts when newer versions are promoted.
  Verified end-to-end with the deterministic `fake_async_video` provider;
  real video providers are **implemented, live-unverified**. ComfyUI is
  not implemented (a future declarative preset).
- **Python AI worker (§5.5 placeholder):** `services/ai-worker/` speaks
  JSON-RPC 2.0 over stdio (`ping`, `health`); AI responsibilities are
  post-MVP.

Documentation:

- [`docs/architecture.md`](docs/architecture.md) — stack, layering, invariants
- [`docs/project-format.md`](docs/project-format.md) — on-disk project layout
- [`docs/recovery.md`](docs/recovery.md) — interrupted-job guarantees
- [`docs/privacy.md`](docs/privacy.md) — credentials, disclosure, redaction
- [`docs/release-checklist.md`](docs/release-checklist.md) — release process

## POST-MVP (not implemented)

The following are explicitly **not** in the current release and must not be
advertised as available: multi-user collaboration, cloud sync, **live-verified
real video providers** (the declarative video presets are implemented but
have never been exercised against a live paid endpoint), ComfyUI, six-panel
layouts, and any feature listed in the master plan's Post-MVP roadmap
(section 60).

## Sprint 1 non-goals (historical)

The following are explicitly out of scope for Sprint 1:

- no AI providers
- no generation
- no Skill Runtime
- no QA
- no scene/video workflow

## Provider credential configuration

Provider API keys are stored in the **operating system credential vault**
(Windows Credential Manager, macOS Keychain, or the Linux Secret Service) —
never in project files or environment variables:

1. Open **Providers** in the desktop app.
2. Choose the provider (for example `openai`, default model `gpt-image-2`).
3. Paste the API key into the password field and click **Save credential**.
4. The key is written to the OS vault and the input is cleared immediately;
   the app only ever shows *configured* / *not configured*.

Notes:

- A one-time migration moves legacy `env://` environment-variable
  references into the vault the first time the variable is present.
- On Linux, a Secret Service implementation (for example `gnome-keyring`)
  must be available.
- Removing a credential clears the database reference first and then deletes
  the vault entry; credentials are never returned to the UI.
