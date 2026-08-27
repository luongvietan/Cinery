# Project Kernel & Asset Versioning Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the local-first desktop substrate for AI Cinematic Production OS so a user can create/reopen a project, import multiple image versions into one asset, inspect them, and transactionally promote one version to canonical without overwriting prior media or losing history.

**Architecture:** Use a Tauri 2 desktop shell with a React + TypeScript + Vite frontend. Keep project metadata and asset-version state in a project-local SQLite database owned by the Tauri/Rust backend; keep media as immutable files under the project directory. Frontend code consumes thin typed Tauri command wrappers and contains presentation state only; persistence invariants such as canonical promotion and version allocation are enforced in backend services and SQLite transactions.

**Tech Stack:** Tauri 2, React, TypeScript, Vite, pnpm workspaces, SQLite via Rust `rusqlite`, Rust `serde`/`thiserror`/`ulid`/`sha2`/`image`, Vitest + React Testing Library for frontend tests, Rust unit/integration tests with `tempfile`.

**Spec:** `docs/specs/ai-cinematic-production-os-master-plan.md`

## Global Constraints

- Implement **only P0 / Sprint 1 plus the minimum P1 Asset Manifest + Versioning behavior required by the Sprint 1 Done Condition**.
- Desktop-first and local-first: the project remains usable without a cloud account or network connection.
- Desktop shell: **Tauri 2**.
- Frontend: **React + TypeScript + Vite**.
- Persistence: **SQLite** for metadata; media files must remain on the local filesystem and must never be stored as SQLite blobs.
- Internal entity IDs are immutable **ULIDs**. Display names and asset aliases are never foreign keys.
- `newest != canonical`: importing a new asset version never promotes it automatically.
- Canonical promotion is explicit and transactional. Promoting a new canonical version supersedes the previous canonical version for the same asset while preserving every file and historical record.
- Never overwrite user media. Imported source files are copied into unique project-managed paths; originals are left untouched.
- No Next.js.
- No cloud authentication, billing, sync, provider adapters, AI generation, Canon Engine, Skill Runtime, visual QA, Scene workflows, Cinema Compiler, collaboration, marketplace, or video editor in this plan.
- Every important state transition has automated tests.
- The application must remain runnable after every task.
- Provider/AI concerns must not appear in the Sprint 1 domain or persistence APIs.
- Do not silently retry destructive or paid actions; Sprint 1 contains no paid actions.
- Use TDD for deterministic domain and persistence behavior and commit after every task.

---

# 1. Sprint 1 Product Context

The final product is a model-agnostic AI filmmaking production environment. Its durable layer is canonical state, asset history, executable workflows, versioning/provenance, validation, and provider portability.

Sprint 1 deliberately builds none of the AI features. It creates the substrate those features will depend on:

```text
LOCAL PROJECT
    ↓
PROJECT DATABASE
    ↓
ASSET
    ↓
IMMUTABLE ASSET VERSIONS
    ↓
EXPLICIT CANONICAL PROMOTION
    ↓
PERSISTENT HISTORY
```

The Sprint 1 user story is:

> “I create a project, import two different face-reference images as V01 and V02 of one Face Lock asset, inspect both, promote V01 to canonical, then promote V02. V01 becomes superseded, V02 becomes canonical, both files remain intact, and the exact state survives application restart.”

This workflow is the minimum proof that the application can manage cinematic canon as durable state instead of relying on filenames or chat memory.

---

# 2. Sprint 1 Domain Decisions

These decisions are fixed for this implementation plan.

## 2.1 Project directory format

A project directory contains:

```text
<project-root>/
├── project.yaml
├── project.db
├── assets/
│   └── <asset-ulid>/
│       └── v001/
│           └── <asset-version-ulid>.<original-extension>
├── thumbnails/
│   └── <asset-ulid>/
│       └── <asset-version-ulid>.webp
├── canon/
├── characters/
├── worlds/
├── props/
├── scenes/
├── prompts/
├── generations/
└── exports/
```

Only `project.yaml`, `project.db`, `assets/`, and `thumbnails/` are used functionally in Sprint 1. The remaining directories are created now so project layout remains deterministic as later subsystems arrive.

## 2.2 `project.yaml`

`project.yaml` is a stable bootstrap marker, not the mutable machine source of truth.

Exact shape:

```yaml
format: ai-cinematic-production-os
project_id: 01J...
schema_version: 1
```

Rules:
- `project_id` never changes.
- `schema_version` is the project container format version, initially `1`.
- Mutable project name lives only in SQLite.
- Opening a project validates that `project.yaml.project_id` matches the SQLite `projects.id`.

## 2.3 Asset and version semantics

An `Asset` is a conceptual slot such as:

```text
MARA-FACE
```

An `AssetVersion` is one concrete immutable media file:

```text
MARA-FACE-V01
MARA-FACE-V02
```

Sprint 1 supports image asset types only, but the enum should reserve the complete MVP vocabulary without implementing its workflows:

```text
face_lock
outfit
character_sheet
world_plate
shot_keyframe
prop_plate
image
video
audio
```

Only image MIME types are accepted by the import command in Sprint 1:
- `image/png`
- `image/jpeg`
- `image/webp`

## 2.4 Imported version initial state

Every manually imported version starts as:

```text
candidate
```

The import path never promotes canon automatically.

## 2.5 Version status vocabulary

Use exactly:

```text
draft
generated
candidate
qa_failed
repairing
approved
canonical
superseded
```

Sprint 1 actively uses:
- `candidate`
- `canonical`
- `superseded`

The remaining values exist to keep the domain contract compatible with the master spec.

## 2.6 Canonical pointer

The `assets` table uses:

```text
canonical_version_id
```

Do not use an ambiguous `current_version_id`.

A canonical version is the source-of-truth version for that asset. An asset may have no canonical version.

## 2.7 Canonical promotion

For one asset, promotion must occur in one SQLite transaction:

1. Verify target version belongs to asset.
2. Read existing `canonical_version_id`.
3. If existing canonical is different, set its version status to `superseded`.
4. Set target version status to `canonical`.
5. Set `assets.canonical_version_id = target_version_id`.
6. Commit.

If any write fails, none of the changes persist.

A previously superseded version may be explicitly promoted again. In that case the currently canonical version becomes superseded and the target becomes canonical.

## 2.8 Version allocation

Version numbers are positive integers scoped to one asset.

The backend allocates them transactionally:

```text
MAX(version_number) + 1
```

UI displays them zero-padded as `V01`, `V02`, etc., but the database stores integer `1`, `2`.

## 2.9 File immutability

Imported files are copied, never moved.

Managed media filename:

```text
<asset-version-ulid>.<normalized-extension>
```

A later version always gets a different file path.

The backend calculates SHA-256 while importing. If the same content hash is already present on the same asset, reject the import with `DuplicateAssetVersion` rather than creating a redundant version.

## 2.10 Thumbnail behavior

Generate a WebP thumbnail with:
- maximum width 512 px;
- maximum height 512 px;
- aspect ratio preserved;
- no upscaling.

Thumbnail generation failure aborts the import before the DB record is committed; temporary files are removed.

---

# 3. File Structure Map

Create this greenfield structure and keep these responsibilities stable through Sprint 1:

```text
/
├── package.json
├── pnpm-workspace.yaml
├── tsconfig.base.json
├── docs/
│   ├── specs/
│   │   └── ai-cinematic-production-os-master-plan.md
│   └── superpowers/
│       └── plans/
│           └── 2026-08-27-project-kernel-asset-versioning.md
├── packages/
│   └── domain/
│       ├── package.json
│       ├── tsconfig.json
│       └── src/
│           ├── index.ts
│           ├── project.ts
│           ├── asset.ts
│           ├── errors.ts
│           ├── project.test.ts
│           └── asset.test.ts
└── apps/
    └── desktop/
        ├── package.json
        ├── vite.config.ts
        ├── vitest.config.ts
        ├── src/
        │   ├── main.tsx
        │   ├── App.tsx
        │   ├── test/
        │   │   └── setup.ts
        │   ├── lib/
        │   │   └── tauri.ts
        │   ├── features/
        │   │   ├── projects/
        │   │   │   ├── api.ts
        │   │   │   ├── ProjectHome.tsx
        │   │   │   ├── ProjectHome.test.tsx
        │   │   │   ├── ProjectWorkspace.tsx
        │   │   │   └── RecentProjects.tsx
        │   │   └── assets/
        │   │       ├── api.ts
        │   │       ├── AssetList.tsx
        │   │       ├── AssetList.test.tsx
        │   │       ├── AssetInspector.tsx
        │   │       ├── AssetInspector.test.tsx
        │   │       └── ImportAssetVersionButton.tsx
        │   └── styles/
        │       └── app.css
        └── src-tauri/
            ├── Cargo.toml
            ├── tauri.conf.json
            ├── capabilities/
            │   └── default.json
            ├── migrations/
            │   ├── 0001_project_kernel.sql
            │   └── 0002_assets.sql
            └── src/
                ├── main.rs
                ├── lib.rs
                ├── error.rs
                ├── db/
                │   ├── mod.rs
                │   └── migrations.rs
                ├── project/
                │   ├── mod.rs
                │   ├── model.rs
                │   ├── paths.rs
                │   ├── repository.rs
                │   ├── service.rs
                │   ├── recent.rs
                │   └── commands.rs
                └── assets/
                    ├── mod.rs
                    ├── model.rs
                    ├── repository.rs
                    ├── import.rs
                    ├── thumbnail.rs
                    ├── service.rs
                    └── commands.rs
```

Boundary rules:
- `packages/domain`: shared TypeScript DTO/enums/value validation used by frontend; no Tauri or SQLite dependencies.
- `src-tauri/db`: database connection and migration mechanics only.
- `src-tauri/project`: project-specific filesystem/database bootstrap and recent-project registry.
- `src-tauri/assets`: asset persistence, import, thumbnails, canonical promotion.
- React feature folders: UI and typed command calls only; no canonical-promotion SQL/state logic.
- Tauri commands remain thin; services carry behavior; repositories carry SQL.

---

# 4. Shared Interfaces

These names are contractually stable within Sprint 1.

## 4.1 TypeScript project types

`packages/domain/src/project.ts`:

```ts
export interface ProjectSummary {
  id: string;
  name: string;
  rootPath: string;
  schemaVersion: number;
  createdAt: string;
  updatedAt: string;
}

export interface CreateProjectInput {
  rootPath: string;
  name: string;
}

export interface OpenProjectInput {
  rootPath: string;
}

export interface RecentProject {
  projectId: string;
  rootPath: string;
  name: string;
  lastOpenedAt: string;
}
```

Validation:
- `name.trim().length` must be between 1 and 120.
- `rootPath.trim()` must not be empty.

## 4.2 TypeScript asset types

`packages/domain/src/asset.ts`:

```ts
export const ASSET_TYPES = [
  "face_lock",
  "outfit",
  "character_sheet",
  "world_plate",
  "shot_keyframe",
  "prop_plate",
  "image",
  "video",
  "audio",
] as const;

export type AssetType = (typeof ASSET_TYPES)[number];

export const ASSET_VERSION_STATUSES = [
  "draft",
  "generated",
  "candidate",
  "qa_failed",
  "repairing",
  "approved",
  "canonical",
  "superseded",
] as const;

export type AssetVersionStatus =
  (typeof ASSET_VERSION_STATUSES)[number];

export interface Asset {
  id: string;
  projectId: string;
  type: AssetType;
  label: string;
  ownerEntityId: string | null;
  canonicalVersionId: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface AssetVersion {
  id: string;
  assetId: string;
  versionNumber: number;
  status: AssetVersionStatus;
  filePath: string;
  thumbnailPath: string;
  sha256: string;
  originalFilename: string;
  mimeType: "image/png" | "image/jpeg" | "image/webp";
  byteSize: number;
  parentVersionId: string | null;
  createdAt: string;
}

export interface AssetWithVersions {
  asset: Asset;
  versions: AssetVersion[];
}

export interface CreateAssetInput {
  projectRootPath: string;
  type: AssetType;
  label: string;
  ownerEntityId?: string | null;
}

export interface ImportAssetVersionInput {
  projectRootPath: string;
  assetId: string;
  sourcePath: string;
  parentVersionId?: string | null;
}

export interface PromoteAssetVersionInput {
  projectRootPath: string;
  assetVersionId: string;
}

export interface CanonicalPromotionResult {
  asset: Asset;
  promotedVersion: AssetVersion;
  supersededVersionId: string | null;
}
```

Validation:
- `label.trim().length` must be between 1 and 160.
- Sprint 1 `createAsset` rejects `video` and `audio` with `UnsupportedAssetTypeForSprint`.
- `parentVersionId`, when supplied, must belong to the same asset.

## 4.3 Frontend command wrappers

`apps/desktop/src/features/projects/api.ts`:

```ts
export function createProject(
  input: CreateProjectInput,
): Promise<ProjectSummary>;

export function openProject(
  input: OpenProjectInput,
): Promise<ProjectSummary>;

export function listRecentProjects(): Promise<RecentProject[]>;
```

`apps/desktop/src/features/assets/api.ts`:

```ts
export function createAsset(
  input: CreateAssetInput,
): Promise<Asset>;

export function importAssetVersion(
  input: ImportAssetVersionInput,
): Promise<AssetVersion>;

export function listAssets(
  projectRootPath: string,
): Promise<Asset[]>;

export function getAssetWithVersions(
  projectRootPath: string,
  assetId: string,
): Promise<AssetWithVersions>;

export function promoteAssetVersion(
  input: PromoteAssetVersionInput,
): Promise<CanonicalPromotionResult>;
```

These wrappers call `invoke()` using matching snake_case Tauri command names.

---

# 5. SQLite Schema

## `0001_project_kernel.sql`

```sql
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS schema_migrations (
  version INTEGER PRIMARY KEY,
  applied_at TEXT NOT NULL
);

CREATE TABLE projects (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  schema_version INTEGER NOT NULL
);
```

The migration runner records migration version `1` after this SQL executes.

## `0002_assets.sql`

```sql
CREATE TABLE assets (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  type TEXT NOT NULL CHECK (
    type IN (
      'face_lock',
      'outfit',
      'character_sheet',
      'world_plate',
      'shot_keyframe',
      'prop_plate',
      'image',
      'video',
      'audio'
    )
  ),
  label TEXT NOT NULL,
  owner_entity_id TEXT,
  canonical_version_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (project_id) REFERENCES projects(id)
);

CREATE TABLE asset_versions (
  id TEXT PRIMARY KEY,
  asset_id TEXT NOT NULL,
  version_number INTEGER NOT NULL CHECK (version_number > 0),
  status TEXT NOT NULL CHECK (
    status IN (
      'draft',
      'generated',
      'candidate',
      'qa_failed',
      'repairing',
      'approved',
      'canonical',
      'superseded'
    )
  ),
  file_path TEXT NOT NULL,
  thumbnail_path TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  original_filename TEXT NOT NULL,
  mime_type TEXT NOT NULL CHECK (
    mime_type IN ('image/png', 'image/jpeg', 'image/webp')
  ),
  byte_size INTEGER NOT NULL CHECK (byte_size >= 0),
  parent_version_id TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY (asset_id) REFERENCES assets(id),
  FOREIGN KEY (parent_version_id) REFERENCES asset_versions(id),
  UNIQUE(asset_id, version_number),
  UNIQUE(asset_id, sha256)
);

CREATE INDEX idx_asset_versions_asset_id
  ON asset_versions(asset_id);

CREATE INDEX idx_assets_project_id
  ON assets(project_id);
```

SQLite cannot safely create the `assets.canonical_version_id → asset_versions.id` cyclic foreign key in this two-table creation order without extra migration complexity. Sprint 1 enforces canonical-pointer integrity in the repository transaction and verifies it in tests. A later schema hardening migration may rebuild `assets` with the foreign key if needed; do not add that work to Sprint 1.

---

# 6. Error Contract

`apps/desktop/src-tauri/src/error.rs` defines a serializable application error:

```rust
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Project name must contain 1 to 120 characters")]
    InvalidProjectName,

    #[error("Project path is empty")]
    InvalidProjectPath,

    #[error("Project directory is not empty")]
    ProjectDirectoryNotEmpty,

    #[error("Directory is not an AI Cinematic Production OS project")]
    InvalidProjectDirectory,

    #[error("Project manifest does not match project database")]
    ProjectIdentityMismatch,

    #[error("Asset was not found")]
    AssetNotFound,

    #[error("Asset version was not found")]
    AssetVersionNotFound,

    #[error("Parent version does not belong to the target asset")]
    ParentVersionMismatch,

    #[error("This asset type is not supported in Sprint 1")]
    UnsupportedAssetTypeForSprint,

    #[error("Only PNG, JPEG, and WebP images can be imported in Sprint 1")]
    UnsupportedImageFormat,

    #[error("This exact media file is already a version of the asset")]
    DuplicateAssetVersion,

    #[error("Asset version does not belong to the selected asset")]
    AssetVersionOwnershipMismatch,

    #[error("Filesystem operation failed: {0}")]
    FileSystem(String),

    #[error("Database operation failed: {0}")]
    Database(String),

    #[error("Image processing failed: {0}")]
    ImageProcessing(String),
}
```

Expose errors to the frontend as:

```ts
export interface AppCommandError {
  code: string;
  message: string;
}
```

`code` is a stable SCREAMING_SNAKE_CASE identifier derived from the Rust enum variant, for example `DUPLICATE_ASSET_VERSION`.

---

# 7. Task Plan

## Task 1: Bootstrap the desktop workspace and establish testable boundaries

**Files:**
- Create: `package.json`
- Create: `pnpm-workspace.yaml`
- Create: `tsconfig.base.json`
- Create: `packages/domain/package.json`
- Create: `packages/domain/tsconfig.json`
- Create: `packages/domain/src/index.ts`
- Create: `apps/desktop/` using a Tauri 2 + React + TypeScript + Vite scaffold
- Create: `apps/desktop/vitest.config.ts`
- Create: `apps/desktop/src/test/setup.ts`
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Copy spec: `docs/specs/ai-cinematic-production-os-master-plan.md`
- Save this plan: `docs/superpowers/plans/2026-08-27-project-kernel-asset-versioning.md`
- Test: `apps/desktop/src/App.test.tsx`

**Interfaces:**
- Consumes: none.
- Produces: runnable Tauri shell; pnpm workspace; `@cinematic/domain` workspace package; frontend and Rust test commands used by all later tasks.

- [ ] **Step 1: Create the pnpm workspace and scaffold the Tauri 2 React TypeScript desktop app**

Run:

```bash
mkdir -p apps packages/domain/src docs/specs docs/superpowers/plans
cat > pnpm-workspace.yaml <<'YAML'
packages:
  - "apps/*"
  - "packages/*"
YAML

cat > package.json <<'JSON'
{
  "name": "ai-cinematic-production-os",
  "private": true,
  "scripts": {
    "dev": "pnpm --filter @cinematic/desktop tauri dev",
    "test": "pnpm -r test",
    "test:rust": "cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml"
  }
}
JSON

pnpm create tauri-app@latest apps/desktop --template react-ts --manager pnpm
```

After scaffolding, change `apps/desktop/package.json` package name to:

```json
{
  "name": "@cinematic/desktop"
}
```

Create `packages/domain/package.json`:

```json
{
  "name": "@cinematic/domain",
  "version": "0.0.0",
  "private": true,
  "type": "module",
  "main": "./src/index.ts",
  "types": "./src/index.ts",
  "scripts": {
    "test": "vitest run"
  },
  "devDependencies": {
    "typescript": "^5",
    "vitest": "^3"
  }
}
```

Expected: `pnpm install` completes and workspace package resolution works.

- [ ] **Step 2: Add frontend test infrastructure and write the first failing smoke test**

Create `apps/desktop/src/App.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import App from "./App";

describe("App", () => {
  it("renders the product shell", () => {
    render(<App />);
    expect(
      screen.getByRole("heading", {
        name: "AI Cinematic Production OS",
      }),
    ).toBeInTheDocument();
  });
});
```

Create `apps/desktop/src/test/setup.ts`:

```ts
import "@testing-library/jest-dom/vitest";
```

Configure `apps/desktop/vitest.config.ts`:

```ts
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    setupFiles: "./src/test/setup.ts",
  },
});
```

Install:

```bash
pnpm --filter @cinematic/desktop add -D vitest jsdom @testing-library/react @testing-library/jest-dom
```

Run:

```bash
pnpm --filter @cinematic/desktop test
```

Expected: FAIL because the scaffolded `App.tsx` does not yet expose the required heading.

- [ ] **Step 3: Replace the scaffold UI with the minimal product shell**

Replace `apps/desktop/src/App.tsx` with:

```tsx
export default function App() {
  return (
    <main>
      <h1>AI Cinematic Production OS</h1>
      <p>Local-first cinematic project workspace.</p>
    </main>
  );
}
```

Run:

```bash
pnpm --filter @cinematic/desktop test
```

Expected: PASS.

- [ ] **Step 4: Establish the Rust crate dependencies required by Sprint 1**

Add to `apps/desktop/src-tauri/Cargo.toml` dependencies compatible with Tauri 2:

```toml
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
rusqlite = { version = "0.32", features = ["bundled"] }
ulid = "1"
sha2 = "0.10"
image = { version = "0.25", default-features = false, features = ["png", "jpeg", "webp"] }
mime_guess = "2"
chrono = { version = "0.4", features = ["serde"] }
serde_yaml = "0.9"
tauri-plugin-dialog = "2"

[dev-dependencies]
tempfile = "3"
```

Register `tauri-plugin-dialog` in `apps/desktop/src-tauri/src/lib.rs`:

```rust
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

Run:

```bash
pnpm --filter @cinematic/desktop tauri build --debug
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
```

Expected: desktop builds; Rust tests complete with zero failures.

- [ ] **Step 5: Copy the approved master spec into the repository and commit the bootstrap**

Place the supplied master spec at:

```text
docs/specs/ai-cinematic-production-os-master-plan.md
```

Place this plan at:

```text
docs/superpowers/plans/2026-08-27-project-kernel-asset-versioning.md
```

Commit:

```bash
git add .
git commit -m "chore: bootstrap cinematic desktop workspace"
```

**Task 1 acceptance:** `pnpm test`, Rust tests, and a debug Tauri build all pass; the desktop app renders the product heading.

---

## Task 2: Implement project bootstrap, SQLite migrations, create/open, and recent-project persistence

**Files:**
- Create: `packages/domain/src/project.ts`
- Create: `packages/domain/src/project.test.ts`
- Modify: `packages/domain/src/index.ts`
- Create: `apps/desktop/src-tauri/migrations/0001_project_kernel.sql`
- Create: `apps/desktop/src-tauri/src/error.rs`
- Create: `apps/desktop/src-tauri/src/db/mod.rs`
- Create: `apps/desktop/src-tauri/src/db/migrations.rs`
- Create: `apps/desktop/src-tauri/src/project/mod.rs`
- Create: `apps/desktop/src-tauri/src/project/model.rs`
- Create: `apps/desktop/src-tauri/src/project/paths.rs`
- Create: `apps/desktop/src-tauri/src/project/repository.rs`
- Create: `apps/desktop/src-tauri/src/project/service.rs`
- Create: `apps/desktop/src-tauri/src/project/recent.rs`
- Create: `apps/desktop/src-tauri/src/project/commands.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Test: Rust unit tests inside `project/service.rs`, `project/paths.rs`, `project/recent.rs`, and `db/migrations.rs`

**Interfaces:**
- Consumes: Tauri shell from Task 1.
- Produces:
  - `create_project(root_path: String, name: String) -> Result<ProjectSummary, AppCommandError>`
  - `open_project(root_path: String) -> Result<ProjectSummary, AppCommandError>`
  - `list_recent_projects() -> Result<Vec<RecentProject>, AppCommandError>`
  - project-local SQLite database and deterministic project directory layout.

- [ ] **Step 1: Write TypeScript domain validation tests for project input**

Create `packages/domain/src/project.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  validateProjectName,
  validateProjectRootPath,
} from "./project";

describe("project input validation", () => {
  it("rejects blank project names", () => {
    expect(() => validateProjectName("   ")).toThrow(
      "Project name must contain 1 to 120 characters",
    );
  });

  it("rejects names longer than 120 characters", () => {
    expect(() => validateProjectName("x".repeat(121))).toThrow(
      "Project name must contain 1 to 120 characters",
    );
  });

  it("trims a valid project name", () => {
    expect(validateProjectName("  Red Door  ")).toBe("Red Door");
  });

  it("rejects an empty project path", () => {
    expect(() => validateProjectRootPath(" ")).toThrow(
      "Project path is empty",
    );
  });
});
```

Run:

```bash
pnpm --filter @cinematic/domain test
```

Expected: FAIL because validation functions do not exist.

- [ ] **Step 2: Implement project DTOs and validation**

Create `packages/domain/src/project.ts` with the interfaces from Section 4.1 plus:

```ts
export function validateProjectName(value: string): string {
  const trimmed = value.trim();
  if (trimmed.length < 1 || trimmed.length > 120) {
    throw new Error("Project name must contain 1 to 120 characters");
  }
  return trimmed;
}

export function validateProjectRootPath(value: string): string {
  const trimmed = value.trim();
  if (!trimmed) {
    throw new Error("Project path is empty");
  }
  return trimmed;
}
```

Export from `packages/domain/src/index.ts`.

Run:

```bash
pnpm --filter @cinematic/domain test
```

Expected: PASS.

- [ ] **Step 3: Write failing Rust tests for project directory creation and migration bootstrap**

In `apps/desktop/src-tauri/src/project/service.rs`, add tests before implementation:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn creates_project_manifest_database_and_directories() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("red-door");
        std::fs::create_dir(&root).unwrap();

        let project = ProjectService::create(&root, "Red Door").unwrap();

        assert_eq!(project.name, "Red Door");
        assert!(root.join("project.yaml").exists());
        assert!(root.join("project.db").exists());
        assert!(root.join("assets").is_dir());
        assert!(root.join("thumbnails").is_dir());
        assert!(root.join("canon").is_dir());
        assert!(root.join("characters").is_dir());
        assert!(root.join("worlds").is_dir());
        assert!(root.join("props").is_dir());
        assert!(root.join("scenes").is_dir());
        assert!(root.join("prompts").is_dir());
        assert!(root.join("generations").is_dir());
        assert!(root.join("exports").is_dir());
    }

    #[test]
    fn rejects_non_empty_non_project_directory() {
        let temp = tempdir().unwrap();
        std::fs::write(temp.path().join("existing.txt"), b"data").unwrap();

        let error = ProjectService::create(temp.path(), "Red Door")
            .unwrap_err();

        assert!(matches!(
            error,
            AppError::ProjectDirectoryNotEmpty
        ));
    }

    #[test]
    fn reopens_created_project_with_same_identity() {
        let temp = tempdir().unwrap();

        let created = ProjectService::create(temp.path(), "Red Door")
            .unwrap();
        let opened = ProjectService::open(temp.path()).unwrap();

        assert_eq!(created.id, opened.id);
        assert_eq!(created.name, opened.name);
    }
}
```

Run:

```bash
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml project::service
```

Expected: FAIL because `ProjectService` does not exist.

- [ ] **Step 4: Implement migration runner and project repository**

Create `apps/desktop/src-tauri/migrations/0001_project_kernel.sql` using the SQL from Section 5.

Implement `db/migrations.rs` with an explicit static migration list:

```rust
pub struct Migration {
    pub version: i64,
    pub sql: &'static str,
}

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: include_str!("../../migrations/0001_project_kernel.sql"),
    },
];
```

Implement:

```rust
pub fn run_migrations(
    conn: &mut rusqlite::Connection,
) -> Result<(), AppError>
```

Behavior:
- create `schema_migrations` if missing;
- read applied versions;
- run each pending migration in an SQLite transaction;
- insert migration version with UTC timestamp;
- rollback if SQL or insert fails.

Implement repository functions:

```rust
pub fn insert_project(
    conn: &rusqlite::Connection,
    project: &ProjectRecord,
) -> Result<(), AppError>;

pub fn read_project(
    conn: &rusqlite::Connection,
) -> Result<ProjectRecord, AppError>;
```

Run:

```bash
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml db::migrations
```

Expected: PASS.

- [ ] **Step 5: Implement deterministic project filesystem creation and manifest validation**

`project/paths.rs` exposes:

```rust
pub const PROJECT_FORMAT: &str = "ai-cinematic-production-os";
pub const PROJECT_SCHEMA_VERSION: u32 = 1;

pub fn ensure_empty_or_new_directory(
    root: &Path,
) -> Result<(), AppError>;

pub fn create_project_directories(
    root: &Path,
) -> Result<(), AppError>;

pub fn write_manifest(
    root: &Path,
    project_id: &str,
) -> Result<(), AppError>;

pub fn read_manifest(
    root: &Path,
) -> Result<ProjectManifest, AppError>;
```

`ProjectManifest`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub format: String,
    pub project_id: String,
    pub schema_version: u32,
}
```

Write `project.yaml` via a temporary file in the same directory then atomic rename:

```text
project.yaml.tmp → project.yaml
```

Validate:
- exact `format`;
- schema version `1`;
- non-empty project ID.

Run project path tests.

Expected: PASS.

- [ ] **Step 6: Implement `ProjectService::create` and `ProjectService::open`**

Exact signatures:

```rust
pub struct ProjectService;

impl ProjectService {
    pub fn create(
        root: &Path,
        name: &str,
    ) -> Result<ProjectSummary, AppError>;

    pub fn open(
        root: &Path,
    ) -> Result<ProjectSummary, AppError>;
}
```

Create behavior:
1. validate trimmed name;
2. validate directory is empty;
3. generate ULID;
4. create deterministic directories;
5. create/open `project.db`;
6. run migrations;
7. insert project row;
8. write manifest;
9. return summary.

Open behavior:
1. read manifest;
2. open DB;
3. run migrations;
4. read project row;
5. require DB project ID == manifest ID;
6. return summary.

If create fails after creating files, remove files/directories that did not exist before the create attempt. Do not delete a directory supplied by the user.

Run:

```bash
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml project::service
```

Expected: all project tests PASS.

- [ ] **Step 7: Add recent-project registry with deterministic JSON persistence**

Store global recent-project file in the Tauri application config directory as:

```text
recent-projects.json
```

JSON shape:

```json
{
  "projects": [
    {
      "projectId": "01J...",
      "rootPath": "/absolute/path/red-door",
      "name": "Red Door",
      "lastOpenedAt": "2026-08-27T13:00:00Z"
    }
  ]
}
```

Rules:
- most recently opened first;
- one entry per `projectId`;
- maximum 20 entries;
- stale paths remain visible until an open attempt fails; the UI can remove them in a later sprint.

Implement:

```rust
pub fn record_recent_project(
    config_dir: &Path,
    project: &ProjectSummary,
) -> Result<(), AppError>;

pub fn list_recent_projects(
    config_dir: &Path,
) -> Result<Vec<RecentProject>, AppError>;
```

Write with temporary-file + atomic rename.

Tests:
- duplicate project ID updates instead of appending;
- ordering is newest first;
- 21st entry evicts oldest.

Run:

```bash
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml project::recent
```

Expected: PASS.

- [ ] **Step 8: Expose thin Tauri commands and register modules**

Tauri commands:

```rust
#[tauri::command]
pub fn create_project(
    app: tauri::AppHandle,
    root_path: String,
    name: String,
) -> Result<ProjectSummary, AppCommandError>;

#[tauri::command]
pub fn open_project(
    app: tauri::AppHandle,
    root_path: String,
) -> Result<ProjectSummary, AppCommandError>;

#[tauri::command]
pub fn list_recent_projects(
    app: tauri::AppHandle,
) -> Result<Vec<RecentProject>, AppCommandError>;
```

On successful create/open, call `record_recent_project`.

Register commands in `lib.rs`.

Run:

```bash
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
pnpm --filter @cinematic/desktop tauri build --debug
```

Expected: PASS.

- [ ] **Step 9: Commit project kernel**

```bash
git add packages/domain apps/desktop/src-tauri
git commit -m "feat: add local project kernel"
```

**Task 2 acceptance:** service tests prove create/open identity persistence; project directory and DB are deterministic; recent-project registry works; Tauri commands compile.

---

## Task 3: Build the project home UI for create, open, and recent projects

**Files:**
- Create: `apps/desktop/src/lib/tauri.ts`
- Create: `apps/desktop/src/features/projects/api.ts`
- Create: `apps/desktop/src/features/projects/ProjectHome.tsx`
- Create: `apps/desktop/src/features/projects/ProjectHome.test.tsx`
- Create: `apps/desktop/src/features/projects/RecentProjects.tsx`
- Create: `apps/desktop/src/features/projects/ProjectWorkspace.tsx`
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/styles/app.css`
- Modify: `apps/desktop/package.json`

**Interfaces:**
- Consumes:
  - `create_project`
  - `open_project`
  - `list_recent_projects`
  - `ProjectSummary`
  - `RecentProject`
- Produces: a UI that selects/creates a project and transitions into a project workspace carrying the exact absolute `rootPath`.

- [ ] **Step 1: Write failing UI tests for empty home, recent project, and open workspace**

Mock project APIs and create tests:

```tsx
it("shows create and open actions", async () => {
  render(<ProjectHome onProjectOpened={vi.fn()} />);
  expect(
    screen.getByRole("button", { name: "Create Project" }),
  ).toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: "Open Project" }),
  ).toBeInTheDocument();
});

it("opens a recent project into the workspace", async () => {
  vi.mocked(listRecentProjects).mockResolvedValue([
    {
      projectId: "01JRECENT",
      rootPath: "/projects/red-door",
      name: "Red Door",
      lastOpenedAt: "2026-08-27T06:00:00Z",
    },
  ]);

  vi.mocked(openProject).mockResolvedValue({
    id: "01JRECENT",
    rootPath: "/projects/red-door",
    name: "Red Door",
    schemaVersion: 1,
    createdAt: "2026-08-27T05:00:00Z",
    updatedAt: "2026-08-27T05:00:00Z",
  });

  render(<ProjectHome onProjectOpened={vi.fn()} />);
  expect(await screen.findByText("Red Door")).toBeInTheDocument();
});
```

Run:

```bash
pnpm --filter @cinematic/desktop test
```

Expected: FAIL because project feature components do not exist.

- [ ] **Step 2: Implement typed Tauri wrappers**

`apps/desktop/src/lib/tauri.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";

export async function invokeCommand<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  return invoke<T>(command, args);
}
```

`features/projects/api.ts` calls the exact commands from Task 2.

No component calls `invoke()` directly.

- [ ] **Step 3: Implement ProjectHome and RecentProjects**

ProjectHome UI requirements:
- heading `AI Cinematic Production OS`;
- buttons `Create Project` and `Open Project`;
- `Recent Projects` section;
- error banner with command error message;
- disable the active action while a command is in flight.

Use `@tauri-apps/plugin-dialog`:
- Create: select an existing empty directory, then ask project name in an inline form before command execution.
- Open: select a directory, then call `openProject`.

Do not create custom filesystem APIs in React.

- [ ] **Step 4: Implement ProjectWorkspace shell**

Render:

```tsx
<header>
  <h1>{project.name}</h1>
  <span>{project.rootPath}</span>
</header>
<nav>
  <button>Assets</button>
</nav>
<section aria-label="Project workspace" />
```

Keep routing local to `App.tsx` for Sprint 1; do not add a full router until multiple feature routes require one.

- [ ] **Step 5: Run frontend tests and desktop smoke build**

```bash
pnpm --filter @cinematic/desktop test
pnpm --filter @cinematic/desktop tauri build --debug
```

Expected: PASS.

- [ ] **Step 6: Commit project UI**

```bash
git add apps/desktop/src apps/desktop/package.json
git commit -m "feat: add project create and open workspace"
```

**Task 3 acceptance:** user can create/open from the desktop UI and see the selected project's name/root path; recent-project entries render and can be reopened.

---

## Task 4: Implement asset domain, SQLite schema, and asset creation

**Files:**
- Create: `packages/domain/src/asset.ts`
- Create: `packages/domain/src/asset.test.ts`
- Modify: `packages/domain/src/index.ts`
- Create: `apps/desktop/src-tauri/migrations/0002_assets.sql`
- Modify: `apps/desktop/src-tauri/src/db/migrations.rs`
- Create: `apps/desktop/src-tauri/src/assets/mod.rs`
- Create: `apps/desktop/src-tauri/src/assets/model.rs`
- Create: `apps/desktop/src-tauri/src/assets/repository.rs`
- Create: `apps/desktop/src-tauri/src/assets/service.rs`
- Create: `apps/desktop/src-tauri/src/assets/commands.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Test: `packages/domain/src/asset.test.ts`
- Test: Rust tests in `assets/service.rs` and `assets/repository.rs`

**Interfaces:**
- Consumes: project DB/opening primitives from Task 2.
- Produces:
  - `create_asset`
  - `list_assets`
  - `get_asset_with_versions`
  - stable asset/status enums.

- [ ] **Step 1: Write failing TypeScript tests for asset validation and display version formatting**

`packages/domain/src/asset.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  formatVersionNumber,
  validateAssetLabel,
  validateSprintOneAssetType,
} from "./asset";

describe("asset domain", () => {
  it("formats version numbers for display", () => {
    expect(formatVersionNumber(1)).toBe("V01");
    expect(formatVersionNumber(12)).toBe("V12");
    expect(formatVersionNumber(105)).toBe("V105");
  });

  it("trims a valid label", () => {
    expect(validateAssetLabel("  MARA-FACE  ")).toBe("MARA-FACE");
  });

  it("rejects video and audio in Sprint 1", () => {
    expect(() => validateSprintOneAssetType("video")).toThrow(
      "This asset type is not supported in Sprint 1",
    );
    expect(() => validateSprintOneAssetType("audio")).toThrow(
      "This asset type is not supported in Sprint 1",
    );
  });
});
```

Run:

```bash
pnpm --filter @cinematic/domain test
```

Expected: FAIL.

- [ ] **Step 2: Implement asset domain types and validation**

Use the exact interfaces from Section 4.2 plus:

```ts
export function formatVersionNumber(value: number): string {
  return `V${String(value).padStart(2, "0")}`;
}

export function validateAssetLabel(value: string): string {
  const trimmed = value.trim();
  if (trimmed.length < 1 || trimmed.length > 160) {
    throw new Error("Asset label must contain 1 to 160 characters");
  }
  return trimmed;
}

export function validateSprintOneAssetType(
  value: AssetType,
): AssetType {
  if (value === "video" || value === "audio") {
    throw new Error(
      "This asset type is not supported in Sprint 1",
    );
  }
  return value;
}
```

Run domain tests. Expected: PASS.

- [ ] **Step 3: Write failing Rust tests for asset creation and query**

Tests:

```rust
#[test]
fn creates_face_lock_asset_without_canonical_version() {
    let fixture = ProjectFixture::new();
    let asset = AssetService::create_asset(
        &fixture.root,
        "face_lock",
        "MARA-FACE",
        None,
    ).unwrap();

    assert_eq!(asset.asset_type, "face_lock");
    assert_eq!(asset.label, "MARA-FACE");
    assert!(asset.canonical_version_id.is_none());
}

#[test]
fn lists_assets_for_only_the_open_project() {
    let first = ProjectFixture::new();
    let second = ProjectFixture::new();

    AssetService::create_asset(
        &first.root,
        "face_lock",
        "MARA-FACE",
        None,
    ).unwrap();

    AssetService::create_asset(
        &second.root,
        "face_lock",
        "OTHER-FACE",
        None,
    ).unwrap();

    let assets = AssetService::list_assets(&first.root).unwrap();

    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].label, "MARA-FACE");
}
```

Run:

```bash
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml assets
```

Expected: FAIL.

- [ ] **Step 4: Add asset migration and repository**

Add migration version `2` to `MIGRATIONS`.

Repository exact functions:

```rust
pub fn insert_asset(
    conn: &rusqlite::Connection,
    record: &AssetRecord,
) -> Result<(), AppError>;

pub fn list_assets(
    conn: &rusqlite::Connection,
    project_id: &str,
) -> Result<Vec<AssetRecord>, AppError>;

pub fn get_asset(
    conn: &rusqlite::Connection,
    asset_id: &str,
) -> Result<AssetRecord, AppError>;

pub fn list_asset_versions(
    conn: &rusqlite::Connection,
    asset_id: &str,
) -> Result<Vec<AssetVersionRecord>, AppError>;
```

Always order versions by `version_number DESC`.

- [ ] **Step 5: Implement AssetService create/list/get**

Exact methods:

```rust
pub fn create_asset(
    project_root: &Path,
    asset_type: &str,
    label: &str,
    owner_entity_id: Option<String>,
) -> Result<AssetDto, AppError>;

pub fn list_assets(
    project_root: &Path,
) -> Result<Vec<AssetDto>, AppError>;

pub fn get_asset_with_versions(
    project_root: &Path,
    asset_id: &str,
) -> Result<AssetWithVersionsDto, AppError>;
```

Validation:
- trim label;
- label length 1–160;
- reject `video` and `audio`;
- require asset type is in declared enum;
- get project ID from DB, never from frontend-provided ID.

Run:

```bash
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml assets
```

Expected: PASS.

- [ ] **Step 6: Add and register Tauri asset commands**

Commands:
- `create_asset`
- `list_assets`
- `get_asset_with_versions`

Run Rust tests and Tauri debug build.

- [ ] **Step 7: Commit asset domain and persistence**

```bash
git add packages/domain apps/desktop/src-tauri
git commit -m "feat: add asset manifest persistence"
```

**Task 4 acceptance:** project can create/list one conceptual Face Lock asset with no canonical version; schema migration survives reopen.

---

## Task 5: Implement immutable image-version import, hashing, and thumbnails

**Files:**
- Create: `apps/desktop/src-tauri/src/assets/import.rs`
- Create: `apps/desktop/src-tauri/src/assets/thumbnail.rs`
- Modify: `apps/desktop/src-tauri/src/assets/repository.rs`
- Modify: `apps/desktop/src-tauri/src/assets/service.rs`
- Modify: `apps/desktop/src-tauri/src/assets/commands.rs`
- Test: Rust tests in `assets/import.rs`, `assets/thumbnail.rs`, and `assets/service.rs`

**Interfaces:**
- Consumes: asset/project persistence from Tasks 2 and 4.
- Produces:
  - `import_asset_version`
  - immutable managed file path
  - SHA-256 duplicate protection
  - WebP thumbnail
  - transactional version allocation.

- [ ] **Step 1: Write failing tests for image validation, immutable copying, and duplicate rejection**

Create test fixtures programmatically using `image` crate so tests do not depend on committed binary files.

Tests:

```rust
#[test]
fn imports_png_as_candidate_version_one() {
    let fixture = AssetFixture::face_asset();
    let source = fixture.write_png("candidate.png", 64, 64);

    let version = AssetService::import_asset_version(
        &fixture.project_root,
        &fixture.asset_id,
        &source,
        None,
    ).unwrap();

    assert_eq!(version.version_number, 1);
    assert_eq!(version.status, "candidate");
    assert!(fixture.project_root.join(&version.file_path).exists());
    assert!(fixture.project_root.join(&version.thumbnail_path).exists());
    assert!(source.exists());
}

#[test]
fn second_distinct_import_becomes_version_two() {
    let fixture = AssetFixture::face_asset();
    let first = fixture.write_png("first.png", 64, 64);
    let second = fixture.write_png_with_pixel(
        "second.png",
        64,
        64,
        [20, 30, 40, 255],
    );

    AssetService::import_asset_version(
        &fixture.project_root,
        &fixture.asset_id,
        &first,
        None,
    ).unwrap();

    let version = AssetService::import_asset_version(
        &fixture.project_root,
        &fixture.asset_id,
        &second,
        None,
    ).unwrap();

    assert_eq!(version.version_number, 2);
}

#[test]
fn rejects_duplicate_content_on_same_asset() {
    let fixture = AssetFixture::face_asset();
    let source = fixture.write_png("same.png", 64, 64);

    AssetService::import_asset_version(
        &fixture.project_root,
        &fixture.asset_id,
        &source,
        None,
    ).unwrap();

    let error = AssetService::import_asset_version(
        &fixture.project_root,
        &fixture.asset_id,
        &source,
        None,
    ).unwrap_err();

    assert!(matches!(error, AppError::DuplicateAssetVersion));
}

#[test]
fn rejects_non_image_input() {
    let fixture = AssetFixture::face_asset();
    let source = fixture.project_root.join("notes.txt");
    std::fs::write(&source, b"not an image").unwrap();

    let error = AssetService::import_asset_version(
        &fixture.project_root,
        &fixture.asset_id,
        &source,
        None,
    ).unwrap_err();

    assert!(matches!(error, AppError::UnsupportedImageFormat));
}
```

Run:

```bash
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml assets
```

Expected: FAIL.

- [ ] **Step 2: Implement MIME/extension normalization and SHA-256**

`assets/import.rs` exposes:

```rust
pub struct InspectedImage {
    pub mime_type: &'static str,
    pub extension: &'static str,
    pub byte_size: u64,
    pub sha256: String,
}

pub fn inspect_image(
    source: &Path,
) -> Result<InspectedImage, AppError>;
```

Do not trust file extension alone. Decode with `image::ImageReader` and map decoded format:
- PNG → `image/png`, `png`
- JPEG → `image/jpeg`, `jpg`
- WebP → `image/webp`, `webp`

Hash the original bytes using SHA-256.

Run import unit tests for format recognition. Expected: PASS for `inspect_image`.

- [ ] **Step 3: Implement thumbnail generation**

`assets/thumbnail.rs`:

```rust
pub fn generate_thumbnail(
    source: &Path,
    destination: &Path,
) -> Result<(), AppError>;
```

Algorithm:
1. decode image;
2. call `thumbnail(512, 512)`;
3. create destination parent directories;
4. write WebP to a temporary sibling path;
5. rename temporary file to final path.

Test:
- 1024×512 becomes 512×256;
- 200×100 remains 200×100;
- output decodes as WebP.

Run:

```bash
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml thumbnail
```

Expected: PASS.

- [ ] **Step 4: Add repository primitives for transactional version allocation**

Repository functions:

```rust
pub fn find_version_by_hash(
    tx: &rusqlite::Transaction<'_>,
    asset_id: &str,
    sha256: &str,
) -> Result<Option<AssetVersionRecord>, AppError>;

pub fn next_version_number(
    tx: &rusqlite::Transaction<'_>,
    asset_id: &str,
) -> Result<i64, AppError>;

pub fn insert_asset_version(
    tx: &rusqlite::Transaction<'_>,
    record: &AssetVersionRecord,
) -> Result<(), AppError>;
```

`next_version_number` SQL:

```sql
SELECT COALESCE(MAX(version_number), 0) + 1
FROM asset_versions
WHERE asset_id = ?1;
```

- [ ] **Step 5: Implement safe import transaction and filesystem cleanup**

`AssetService::import_asset_version` exact behavior:

1. open project DB;
2. verify asset exists;
3. validate optional parent belongs to same asset;
4. inspect image and compute hash;
5. begin immediate SQLite transaction;
6. reject duplicate hash;
7. allocate next version number;
8. generate version ULID;
9. construct relative media and thumbnail paths;
10. copy source to `<final>.tmp`;
11. fsync copied file;
12. rename to final media path;
13. generate thumbnail;
14. insert `asset_versions` row with status `candidate`;
15. commit;
16. return DTO.

If steps 10–15 fail:
- rollback DB transaction;
- remove final/temp managed media created by this attempt;
- remove thumbnail created by this attempt;
- leave source untouched.

Use relative paths in DB, with `/` separators normalized for portability.

Run all asset tests.

Expected: PASS.

- [ ] **Step 6: Expose `import_asset_version` Tauri command**

Command:

```rust
#[tauri::command]
pub fn import_asset_version(
    project_root_path: String,
    asset_id: String,
    source_path: String,
    parent_version_id: Option<String>,
) -> Result<AssetVersionDto, AppCommandError>;
```

Run:

```bash
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
pnpm --filter @cinematic/desktop tauri build --debug
```

Expected: PASS.

- [ ] **Step 7: Commit immutable import pipeline**

```bash
git add apps/desktop/src-tauri
git commit -m "feat: import immutable asset versions"
```

**Task 5 acceptance:** two distinct images import as V01/V02; originals remain; managed files and thumbnails are unique; duplicate content on the same asset is rejected.

---

## Task 6: Add Asset Manifest UI and version inspector

**Files:**
- Create: `apps/desktop/src/features/assets/api.ts`
- Create: `apps/desktop/src/features/assets/AssetList.tsx`
- Create: `apps/desktop/src/features/assets/AssetList.test.tsx`
- Create: `apps/desktop/src/features/assets/AssetInspector.tsx`
- Create: `apps/desktop/src/features/assets/AssetInspector.test.tsx`
- Create: `apps/desktop/src/features/assets/ImportAssetVersionButton.tsx`
- Modify: `apps/desktop/src/features/projects/ProjectWorkspace.tsx`
- Modify: `apps/desktop/src/styles/app.css`

**Interfaces:**
- Consumes:
  - `create_asset`
  - `list_assets`
  - `get_asset_with_versions`
  - `import_asset_version`
- Produces: usable Asset Manifest UI for creating a Face Lock asset, importing V01/V02, selecting versions, and inspecting immutable metadata.

- [ ] **Step 1: Write failing AssetList UI tests**

Mock asset API and test:

```tsx
it("shows an empty asset state", async () => {
  vi.mocked(listAssets).mockResolvedValue([]);
  render(
    <AssetList
      projectRootPath="/projects/red-door"
      onSelectAsset={vi.fn()}
    />,
  );

  expect(
    await screen.findByText("No assets yet"),
  ).toBeInTheDocument();
});

it("shows canonical status without assuming newest is canonical", async () => {
  vi.mocked(listAssets).mockResolvedValue([
    {
      id: "asset-1",
      projectId: "project-1",
      type: "face_lock",
      label: "MARA-FACE",
      ownerEntityId: null,
      canonicalVersionId: "version-1",
      createdAt: "2026-08-27T06:00:00Z",
      updatedAt: "2026-08-27T06:10:00Z",
    },
  ]);

  render(
    <AssetList
      projectRootPath="/projects/red-door"
      onSelectAsset={vi.fn()}
    />,
  );

  expect(await screen.findByText("MARA-FACE"))
    .toBeInTheDocument();
  expect(screen.getByText("Canonical set"))
    .toBeInTheDocument();
});
```

Run frontend tests. Expected: FAIL.

- [ ] **Step 2: Implement typed asset API wrapper**

`features/assets/api.ts` implements the exact Section 4.3 signatures using `invokeCommand`.

No API wrapper should mutate UI state.

- [ ] **Step 3: Implement AssetList and Create Face Asset action**

Sprint 1 UI supports creating these image asset types in a select:
- Face Lock
- Outfit
- Character Sheet
- World Plate
- Shot Keyframe
- Prop Plate
- Image

Required controls:
- `New Asset`
- type selector
- label input
- `Create`

List row shows:
- label;
- humanized asset type;
- `Canonical set` or `No canonical version`.

After create, refresh the list and auto-select the new asset.

- [ ] **Step 4: Write failing AssetInspector tests**

Test version order and metadata:

```tsx
it("renders newest version first but marks canonical explicitly", async () => {
  vi.mocked(getAssetWithVersions).mockResolvedValue({
    asset: {
      id: "asset-1",
      projectId: "project-1",
      type: "face_lock",
      label: "MARA-FACE",
      ownerEntityId: null,
      canonicalVersionId: "v1",
      createdAt: "2026-08-27T06:00:00Z",
      updatedAt: "2026-08-27T06:10:00Z",
    },
    versions: [
      {
        id: "v2",
        assetId: "asset-1",
        versionNumber: 2,
        status: "candidate",
        filePath: "assets/asset-1/v002/v2.png",
        thumbnailPath: "thumbnails/asset-1/v2.webp",
        sha256: "hash2",
        originalFilename: "second.png",
        mimeType: "image/png",
        byteSize: 100,
        parentVersionId: null,
        createdAt: "2026-08-27T06:10:00Z",
      },
      {
        id: "v1",
        assetId: "asset-1",
        versionNumber: 1,
        status: "canonical",
        filePath: "assets/asset-1/v001/v1.png",
        thumbnailPath: "thumbnails/asset-1/v1.webp",
        sha256: "hash1",
        originalFilename: "first.png",
        mimeType: "image/png",
        byteSize: 100,
        parentVersionId: null,
        createdAt: "2026-08-27T06:05:00Z",
      },
    ],
  });

  render(
    <AssetInspector
      projectRootPath="/projects/red-door"
      assetId="asset-1"
    />,
  );

  const versions = await screen.findAllByTestId("asset-version");
  expect(versions[0]).toHaveTextContent("V02");
  expect(versions[0]).toHaveTextContent("Candidate");
  expect(versions[1]).toHaveTextContent("V01");
  expect(versions[1]).toHaveTextContent("Canonical");
});
```

Run. Expected: FAIL.

- [ ] **Step 5: Implement AssetInspector**

Display:
- asset label/type;
- canonical version badge;
- each version newest-first;
- `V##`;
- status;
- thumbnail;
- original filename;
- SHA-256;
- byte size;
- MIME;
- created timestamp;
- managed relative file path.

Use Tauri's asset protocol or `convertFileSrc` to render thumbnail paths under the project root. Keep path joining in a helper; do not concatenate filesystem paths in JSX.

- [ ] **Step 6: Implement ImportAssetVersionButton**

Use native file dialog filtered to:
- PNG
- JPEG
- WebP

After the user selects a file:
- call `importAssetVersion`;
- show command error if import fails;
- refresh AssetInspector;
- do not auto-promote.

Button text:

```text
Import New Version
```

- [ ] **Step 7: Run frontend tests and desktop build**

```bash
pnpm --filter @cinematic/desktop test
pnpm --filter @cinematic/desktop tauri build --debug
```

Expected: PASS.

- [ ] **Step 8: Commit Asset Manifest UI**

```bash
git add apps/desktop/src
git commit -m "feat: add asset manifest and version inspector"
```

**Task 6 acceptance:** from the UI, user can create `MARA-FACE`, import two images, see V02 first because it is newest, and independently see that neither/newest is canonical until explicitly promoted.

---

## Task 7: Implement transactional canonical promotion and superseding

**Files:**
- Modify: `packages/domain/src/asset.ts`
- Modify: `packages/domain/src/asset.test.ts`
- Modify: `apps/desktop/src-tauri/src/assets/repository.rs`
- Modify: `apps/desktop/src-tauri/src/assets/service.rs`
- Modify: `apps/desktop/src-tauri/src/assets/commands.rs`
- Modify: `apps/desktop/src/features/assets/api.ts`
- Modify: `apps/desktop/src/features/assets/AssetInspector.tsx`
- Modify: `apps/desktop/src/features/assets/AssetInspector.test.tsx`
- Test: Rust canonical-promotion tests in `assets/service.rs`

**Interfaces:**
- Consumes: existing asset + version persistence.
- Produces:
  - `promote_asset_version`
  - `CanonicalPromotionResult`
  - explicit `Promote to Canon` UI.
- Invariant: exactly zero or one canonical version per asset after every successful transaction.

- [ ] **Step 1: Write failing Rust tests for first promotion, second promotion, re-promotion, and rollback**

Tests:

```rust
#[test]
fn promotes_candidate_to_canonical() {
    let fixture = AssetFixture::with_two_versions();

    let result = AssetService::promote_asset_version(
        &fixture.project_root,
        &fixture.version_one_id,
    ).unwrap();

    assert_eq!(
        result.asset.canonical_version_id.as_deref(),
        Some(fixture.version_one_id.as_str())
    );
    assert_eq!(result.promoted_version.status, "canonical");
    assert!(result.superseded_version_id.is_none());
}

#[test]
fn promoting_second_version_supersedes_first() {
    let fixture = AssetFixture::with_two_versions();

    AssetService::promote_asset_version(
        &fixture.project_root,
        &fixture.version_one_id,
    ).unwrap();

    let result = AssetService::promote_asset_version(
        &fixture.project_root,
        &fixture.version_two_id,
    ).unwrap();

    let reloaded = AssetService::get_asset_with_versions(
        &fixture.project_root,
        &fixture.asset_id,
    ).unwrap();

    let first = reloaded.versions.iter()
        .find(|v| v.id == fixture.version_one_id)
        .unwrap();
    let second = reloaded.versions.iter()
        .find(|v| v.id == fixture.version_two_id)
        .unwrap();

    assert_eq!(first.status, "superseded");
    assert_eq!(second.status, "canonical");
    assert_eq!(
        result.superseded_version_id.as_deref(),
        Some(fixture.version_one_id.as_str())
    );
}

#[test]
fn superseded_version_can_be_promoted_again() {
    let fixture = AssetFixture::with_two_versions();

    AssetService::promote_asset_version(
        &fixture.project_root,
        &fixture.version_one_id,
    ).unwrap();
    AssetService::promote_asset_version(
        &fixture.project_root,
        &fixture.version_two_id,
    ).unwrap();

    AssetService::promote_asset_version(
        &fixture.project_root,
        &fixture.version_one_id,
    ).unwrap();

    let reloaded = AssetService::get_asset_with_versions(
        &fixture.project_root,
        &fixture.asset_id,
    ).unwrap();

    assert_eq!(
        reloaded.asset.canonical_version_id.as_deref(),
        Some(fixture.version_one_id.as_str())
    );
}

#[test]
fn failed_promotion_does_not_change_existing_canonical() {
    let fixture = AssetFixture::with_two_versions();

    AssetService::promote_asset_version(
        &fixture.project_root,
        &fixture.version_one_id,
    ).unwrap();

    let error = AssetService::promote_asset_version(
        &fixture.project_root,
        "01JNONEXISTENTVERSION",
    ).unwrap_err();

    assert!(matches!(error, AppError::AssetVersionNotFound));

    let reloaded = AssetService::get_asset_with_versions(
        &fixture.project_root,
        &fixture.asset_id,
    ).unwrap();

    assert_eq!(
        reloaded.asset.canonical_version_id.as_deref(),
        Some(fixture.version_one_id.as_str())
    );
}
```

Run:

```bash
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml promote
```

Expected: FAIL.

- [ ] **Step 2: Add repository transaction primitives**

Exact repository function:

```rust
pub fn promote_canonical_version(
    conn: &mut rusqlite::Connection,
    target_version_id: &str,
) -> Result<CanonicalPromotionRecord, AppError>;
```

Implementation:
- begin `TransactionBehavior::Immediate`;
- select target version + asset ID;
- select asset's `canonical_version_id`;
- if current canonical == target ID, return current state without status churn;
- if current canonical exists, update that version status to `superseded`;
- update target version to `canonical`;
- update asset canonical pointer and `updated_at`;
- verify:
  ```sql
  SELECT COUNT(*)
  FROM asset_versions
  WHERE asset_id = ?1 AND status = 'canonical';
  ```
  must equal `1`;
- commit.

Do not update any media paths.

- [ ] **Step 3: Implement service and command**

Service:

```rust
pub fn promote_asset_version(
    project_root: &Path,
    asset_version_id: &str,
) -> Result<CanonicalPromotionDto, AppError>;
```

Tauri command:

```rust
#[tauri::command]
pub fn promote_asset_version(
    project_root_path: String,
    asset_version_id: String,
) -> Result<CanonicalPromotionDto, AppCommandError>;
```

Run Rust tests. Expected: PASS.

- [ ] **Step 4: Write failing UI test for explicit canonical promotion**

Test:

```tsx
it("promotes a candidate only after explicit user action", async () => {
  vi.mocked(promoteAssetVersion).mockResolvedValue({
    asset: canonicalAsset,
    promotedVersion: canonicalVersionTwo,
    supersededVersionId: "v1",
  });

  render(
    <AssetInspector
      projectRootPath="/projects/red-door"
      assetId="asset-1"
    />,
  );

  const promoteButton = await screen.findByRole("button", {
    name: "Promote V02 to Canon",
  });

  await userEvent.click(promoteButton);

  expect(promoteAssetVersion).toHaveBeenCalledWith({
    projectRootPath: "/projects/red-door",
    assetVersionId: "v2",
  });
});
```

Run frontend tests. Expected: FAIL.

- [ ] **Step 5: Add promotion UI**

For every non-canonical version show:

```text
Promote V## to Canon
```

On click:
- show confirmation:
  ```text
  Make V02 the canonical version of MARA-FACE?
  The current canonical version will be preserved and marked Superseded.
  ```
- call API;
- refresh asset details;
- show statuses returned by backend.

Do not infer superseding on the frontend.

- [ ] **Step 6: Run complete tests and desktop build**

```bash
pnpm test
pnpm test:rust
pnpm --filter @cinematic/desktop tauri build --debug
```

Expected: PASS.

- [ ] **Step 7: Commit canonical promotion**

```bash
git add packages/domain apps/desktop
git commit -m "feat: add transactional canonical promotion"
```

**Task 7 acceptance:** V01 can become canonical; promoting V02 marks V01 superseded and V02 canonical; re-promoting V01 reverses the canonical pointer without deleting any version.

---

## Task 8: Prove restart persistence, corruption handling, and Sprint 1 end-to-end acceptance

**Files:**
- Modify: `apps/desktop/src-tauri/src/project/service.rs`
- Modify: `apps/desktop/src-tauri/src/assets/service.rs`
- Create: `apps/desktop/src-tauri/tests/sprint_one_acceptance.rs`
- Create: `docs/superpowers/plans/sprint-1-verification.md`
- Modify: `README.md` if scaffold created one; otherwise Create: `README.md`

**Interfaces:**
- Consumes: complete Sprint 1 project/asset APIs.
- Produces: one automated Rust acceptance test that proves the Done Condition and one human smoke checklist for the desktop UI.

- [ ] **Step 1: Write the failing acceptance test before adding any test-only helpers**

`apps/desktop/src-tauri/tests/sprint_one_acceptance.rs`:

```rust
#[test]
fn sprint_one_project_and_asset_state_survives_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let project = cinematic_app::project::ProjectService::create(
        root,
        "Red Door",
    ).unwrap();

    let asset = cinematic_app::assets::AssetService::create_asset(
        root,
        "face_lock",
        "MARA-FACE",
        None,
    ).unwrap();

    let first_source = test_image(root, "first.png", [10, 20, 30, 255]);
    let second_source = test_image(root, "second.png", [40, 50, 60, 255]);

    let first = cinematic_app::assets::AssetService::import_asset_version(
        root,
        &asset.id,
        &first_source,
        None,
    ).unwrap();

    let second = cinematic_app::assets::AssetService::import_asset_version(
        root,
        &asset.id,
        &second_source,
        None,
    ).unwrap();

    cinematic_app::assets::AssetService::promote_asset_version(
        root,
        &first.id,
    ).unwrap();

    cinematic_app::assets::AssetService::promote_asset_version(
        root,
        &second.id,
    ).unwrap();

    drop(project);

    let reopened =
        cinematic_app::project::ProjectService::open(root).unwrap();

    let reloaded =
        cinematic_app::assets::AssetService::get_asset_with_versions(
            root,
            &asset.id,
        ).unwrap();

    assert_eq!(reopened.name, "Red Door");
    assert_eq!(
        reloaded.asset.canonical_version_id.as_deref(),
        Some(second.id.as_str())
    );

    let v1 = reloaded.versions.iter()
        .find(|v| v.id == first.id)
        .unwrap();
    let v2 = reloaded.versions.iter()
        .find(|v| v.id == second.id)
        .unwrap();

    assert_eq!(v1.status, "superseded");
    assert_eq!(v2.status, "canonical");

    assert!(root.join(&v1.file_path).exists());
    assert!(root.join(&v2.file_path).exists());
    assert!(first_source.exists());
    assert!(second_source.exists());
}
```

Run:

```bash
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test sprint_one_acceptance
```

Expected: FAIL if public module exports/test utilities are incomplete.

- [ ] **Step 2: Expose only the production service modules required by integration tests**

In `src-tauri/src/lib.rs`, export:

```rust
pub mod assets;
pub mod db;
pub mod error;
pub mod project;
```

Keep command wiring inside the same crate; do not expose repositories as public APIs unless integration tests require them.

Create local helper `test_image()` inside the integration test using `image::RgbaImage`.

Run the acceptance test. Expected: PASS.

- [ ] **Step 3: Add project corruption tests**

Add tests:

1. `project.yaml` missing → `InvalidProjectDirectory`.
2. `project.db` missing → `InvalidProjectDirectory`.
3. manifest project ID differs from DB row → `ProjectIdentityMismatch`.
4. invalid project format string → `InvalidProjectDirectory`.

Run:

```bash
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml project
```

Expected: PASS.

- [ ] **Step 4: Verify canonical invariants directly against SQLite**

In acceptance test, query:

```sql
SELECT COUNT(*)
FROM asset_versions
WHERE asset_id = ?1
AND status = 'canonical';
```

Assert result is `1`.

Also assert:

```sql
SELECT COUNT(*)
FROM asset_versions
WHERE asset_id = ?1;
```

is `2`.

This proves canonical promotion did not delete history.

- [ ] **Step 5: Create the human desktop verification checklist**

Create `docs/superpowers/plans/sprint-1-verification.md`:

```markdown
# Sprint 1 Desktop Verification

1. Launch the desktop app.
2. Click Create Project.
3. Choose an empty directory.
4. Name the project `Red Door`.
5. Confirm the workspace opens and displays the project root path.
6. Create an asset:
   - Type: Face Lock
   - Label: `MARA-FACE`
7. Import first PNG.
8. Confirm it appears as V01 / Candidate.
9. Import a different PNG.
10. Confirm it appears as V02 / Candidate and V01 remains visible.
11. Promote V01 to Canon.
12. Confirm V01 shows Canonical.
13. Promote V02 to Canon.
14. Confirm:
    - V02 = Canonical
    - V01 = Superseded
15. Close the app completely.
16. Reopen the app.
17. Open `Red Door` from Recent Projects.
18. Open `MARA-FACE`.
19. Confirm V01 and V02 are both present with the same statuses.
20. Confirm both managed files exist on disk.
21. Confirm the two original imported files still exist and were not modified.
```

- [ ] **Step 6: Document development and verification commands**

`README.md` must contain:

```bash
pnpm install
pnpm dev
pnpm test
pnpm test:rust
pnpm --filter @cinematic/desktop tauri build --debug
```

Also document the Sprint 1 non-goals exactly:
- no AI providers;
- no generation;
- no Canon Engine;
- no Skill Runtime;
- no QA;
- no scene/video workflow.

- [ ] **Step 7: Run the full automated verification suite**

Run:

```bash
pnpm install
pnpm test
pnpm test:rust
pnpm --filter @cinematic/desktop tauri build --debug
```

Expected:
- all TypeScript tests PASS;
- all Rust tests PASS;
- `sprint_one_acceptance` PASS;
- debug desktop build succeeds.

- [ ] **Step 8: Perform the human desktop checklist**

Run:

```bash
pnpm dev
```

Complete every step in `docs/superpowers/plans/sprint-1-verification.md`.

Expected: all 21 verification steps pass without manual filesystem/database repair.

- [ ] **Step 9: Commit Sprint 1 acceptance**

```bash
git add README.md apps/desktop/src-tauri/tests docs/superpowers/plans
git commit -m "test: verify sprint one project persistence"
```

**Task 8 acceptance:** the automated acceptance test and human desktop checklist both prove the exact Sprint 1 Done Condition.

---

# 8. Cross-Task Invariants Checklist

Every task must preserve these invariants:

- [ ] A project is identified by manifest ULID and matching DB row.
- [ ] Mutable project name is read from SQLite, not `project.yaml`.
- [ ] User source media is never moved, renamed, or edited.
- [ ] Managed media file paths are unique per asset version.
- [ ] Asset version records are immutable except for lifecycle `status`.
- [ ] Import always creates `candidate`, never `canonical`.
- [ ] Canonical promotion is explicit.
- [ ] At most one version per asset has `status = canonical` after a successful promotion.
- [ ] `assets.canonical_version_id` matches the version with `status = canonical`.
- [ ] Superseded files remain present and queryable.
- [ ] Reopening the project reconstructs state from disk + SQLite, not frontend memory.
- [ ] Frontend never implements SQL/domain promotion logic.
- [ ] No AI/provider concepts leak into these APIs.

---

# 9. Sprint 1 Definition of Done

Sprint 1 is done only when all statements below are true:

1. A user can create a local project from the desktop UI.
2. Closing and reopening the project preserves project identity and name.
3. A user can create a `face_lock` asset called `MARA-FACE`.
4. A user can import two distinct supported images into that one asset.
5. Those images become V01 and V02 without modifying either source file.
6. Both managed media files and thumbnails exist under the project directory.
7. Both imported versions begin as `candidate`.
8. V02 being newer does not make it canonical.
9. User can explicitly promote V01 to canonical.
10. User can explicitly promote V02 to canonical.
11. Promoting V02 transactionally changes V01 to `superseded`.
12. No version is deleted during promotion.
13. Exactly one canonical version exists for the asset after promotion.
14. Closing and reopening the application preserves V01/V02 statuses and files.
15. Duplicate binary content on the same asset is rejected.
16. Corrupt/mismatched project identity produces an actionable error instead of silently opening.
17. TypeScript tests pass.
18. Rust tests pass.
19. Tauri debug build succeeds.
20. Human verification checklist passes.

No work from P2 Canon Engine or later phases is required for Sprint 1 acceptance.

---

# 10. Self-Review Results

## 10.1 Spec coverage

Covered master-plan Sprint 1 requirements:
- Tauri desktop shell → Task 1.
- React + TypeScript + Vite → Task 1.
- local project create/open → Tasks 2–3.
- SQLite migrations/repositories → Tasks 2 and 4.
- deterministic project filesystem → Task 2.
- asset entities and versioning → Task 4.
- immutable import + hash + thumbnails → Task 5.
- asset inspector → Task 6.
- explicit canonical promotion → Task 7.
- transactional superseding → Task 7.
- preserve historical files → Tasks 5, 7, 8.
- close/reopen persistence → Task 8.
- automated domain/integration tests → every task, with acceptance in Task 8.
- no providers/AI/Canon Engine/Skill Runtime/QA → enforced in Global Constraints and README verification.

No Sprint 1 spec gap remains.

## 10.2 Placeholder scan

Placeholder scan completed against the Writing Plans failure patterns. No deferred implementation markers, vague validation instructions, unspecified test steps, cross-task shorthand, or missing code-step detail remain.

References to future phases appear only as architectural context and do not defer any Sprint 1 requirement.

## 10.3 Type/signature consistency

Checked:
- `ProjectSummary`, `RecentProject`, project command names, and frontend wrappers match.
- `Asset`, `AssetVersion`, `AssetWithVersions`, create/import/promote DTO names match across Sections 4 and Tasks 4–7.
- canonical field is consistently named `canonicalVersionId` in TypeScript and `canonical_version_id` in SQLite/Rust.
- import initial status is consistently `candidate`.
- canonical promotion result consistently contains `asset`, `promotedVersion`, and `supersededVersionId`.
- asset versions are consistently ordered newest-first in query/UI while canonicality remains explicit.
- project root path is carried explicitly through command inputs and never inferred from frontend global state.

No naming/signature mismatch remains.

---

# 11. Execution Handoff

This plan is intentionally limited to P0 / Sprint 1 and the minimum Asset Manifest/Versioning behavior needed to make Sprint 1 independently useful.

After Sprint 1 passes the automated and human Done Conditions, stop. Do not begin Canon Engine work from this plan. Review the actual interfaces and project structure created by Sprint 1, then write a separate Superpowers implementation plan for **P2 — Canon Engine**.

Recommended execution mode:

1. **Subagent-Driven Development — recommended**
   - use `superpowers:subagent-driven-development`;
   - fresh subagent per task;
   - review each task before advancing.

2. **Inline Execution**
   - use `superpowers:executing-plans`;
   - execute in batches with review checkpoints.

At execution time, first ensure an isolated workspace using `superpowers:using-git-worktrees` or verify the current worktree is already isolated.
