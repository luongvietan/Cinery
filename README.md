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

## MVP IMPLEMENTED (P0–P9)

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
- **Visual QA & repair (P6–P7):** check planner, per-check review,
  repair-to-child-version workflow, exact-version scene pinning.
- **Cinema compiler (P8):** provider-neutral scene compilation, runtime
  budget guards, protected-TBD firewall, durable prompt export.
- **Integration & polish (P9):** project overview & readiness, health
  scanning, provenance traversal, unified job lifecycle/recovery,
  privacy hardening, diagnostics export, UX/accessibility polish,
  golden-path acceptance fixture.

Documentation:

- [`docs/architecture.md`](docs/architecture.md) — stack, layering, invariants
- [`docs/project-format.md`](docs/project-format.md) — on-disk project layout
- [`docs/recovery.md`](docs/recovery.md) — interrupted-job guarantees
- [`docs/privacy.md`](docs/privacy.md) — credentials, disclosure, redaction
- [`docs/release-checklist.md`](docs/release-checklist.md) — release process

## POST-MVP (not implemented)

The following are explicitly **not** in the current release and must not be
advertised as available: multi-user collaboration, cloud sync, video
generation providers beyond the MVP set, six-panel layouts, and any feature
listed in the master plan's Post-MVP roadmap (section 60).

## Sprint 1 non-goals (historical)

The following are explicitly out of scope for Sprint 1:

- no AI providers
- no generation
- no Skill Runtime
- no QA
- no scene/video workflow
