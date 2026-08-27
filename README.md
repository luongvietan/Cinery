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

### Sprint 1 non-goals

The following are explicitly out of scope for Sprint 1:

- no AI providers
- no generation
- no Skill Runtime
- no QA
- no scene/video workflow
