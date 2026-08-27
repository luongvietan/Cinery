# Canon Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a structured, revisioned Canon Engine on top of the completed Project Kernel + Asset Versioning substrate. Users must be able to build and lock story canon, characters, locations, factions, world rules, production rules, permanent visual locks, and protected TBDs; inspect section history; close/reopen the project without losing state; and export a deterministic human-readable Story Bible Markdown file.

**Architecture:** Canon is stored as typed structured data in project-local SQLite, not as chat history or mutable Markdown. Each conceptual canon object is a stable `canon_entity`; each independently lockable field is a `canon_section`. Every section mutation is revisioned transactionally. Locked sections are canonical source of truth; draft sections remain editable working state. Protected TBDs are first-class rows that later workflows can query as a firewall. Markdown is generated from SQLite as an export, never used as the mutable machine source of truth.

**Tech Stack:** Existing Tauri 2 + React + TypeScript + Vite + pnpm workspace from Sprint 1; SQLite via `rusqlite`; Rust `serde`/`serde_json`/`ulid`/`chrono`/`thiserror`; TypeScript domain types validated with Zod; Vitest + React Testing Library; Rust unit/integration tests with `tempfile`.

**Prerequisite Plan:** `docs/superpowers/plans/2026-08-27-project-kernel-asset-versioning.md`

**Master Spec:** `docs/specs/ai-cinematic-production-os-master-plan.md`

---

# 0. Entry Criteria

Do not begin this plan until the prior Sprint 1 Done Condition passes.

The existing repository is expected to provide:

```text
Tauri 2 desktop shell
React + TypeScript + Vite frontend
pnpm workspaces

ProjectService
- create
- open

Project-local:
- project.yaml
- project.db
- deterministic directories

AssetService
- create asset
- import immutable versions
- canonical promotion
- superseding
- restart persistence

SQLite migrations:
0001_project_kernel.sql
0002_assets.sql
```

If actual filenames differ slightly because Sprint 1 implementation made a justified change, adapt paths to the actual repository while preserving the domain boundaries and signatures established by Sprint 1.

Do **not** redesign Sprint 1 during this plan unless a concrete failing test proves the existing interface cannot support Canon Engine.

---

# 1. Scope

Implement only **P2 — Canon Engine**.

## Included

- structured Story Canon;
- structured Character Canon;
- Locations;
- Factions;
- World Rules;
- Production Rules;
- section-level draft/locked state;
- revision history for every canon section;
- permanent visual locks as structured Character Canon;
- protected/open/resolved TBD entries;
- project-wide TBD query;
- deterministic Story Bible Markdown export;
- Canon UI;
- restart persistence;
- automated acceptance tests.

## Explicitly excluded

Do not implement:
- LLM interview;
- AI suggestions;
- Story Bible Builder skill execution;
- Skill Runtime;
- Production Router;
- prompt compilation;
- providers;
- image/video generation;
- visual QA;
- Scene generation;
- Cinema Compiler;
- embeddings/vector search;
- cloud sync;
- auth;
- collaboration;
- marketplace;
- asset-to-canon automation;
- automatic canon mutation from imported images;
- automatic visual-lock extraction from images.

This phase is deterministic structured state management.

---

# 2. Canon Rules

These are hard invariants.

## 2.1 Canon is structured data

SQLite is the machine source of truth.

Generated Markdown is an export only.

Never parse `story-bible.md` on project open to reconstruct current state.

## 2.2 Locking is section-level

Users must be able to lock:

```text
Premise
```

without locking:

```text
Thesis
Timeline
Aesthetic
```

The entity itself is only a container.

## 2.3 Locked sections cannot be edited

Mutation of a locked section must fail with:

```text
CANON_SECTION_LOCKED
```

The user must explicitly unlock first.

## 2.4 Every section mutation creates a revision

These actions create revisions:

```text
create
edit
lock
unlock
```

Revision history is append-only.

## 2.5 Revision numbers are section-scoped

For one section:

```text
1
2
3
...
```

No gaps should be created by successful transactions.

## 2.6 Locked means canonical

Later Skill Runtime must consume locked sections by default.

Draft sections are working state, not guaranteed canon.

This plan does not implement Skill Runtime, but the database/API must make the distinction explicit.

## 2.7 No silent deletion

P2 does not delete canon entities or section history.

Entity deletion/archive can be designed later.

## 2.8 Visual locks are structured

Permanent visual rules must not live only inside prose.

Example:

```json
{
  "id": "lock-right-eyebrow-scar",
  "key": "right_eyebrow_scar",
  "description": "Small healed linear scar through the outer third of character-right eyebrow.",
  "severity": "required",
  "validatorHint": "When character faces camera, character-right appears viewer-left."
}
```

These are stored inside the Character Canon `visual_locks` section so later QA can query them without introducing a second mutable source of truth.

## 2.9 Protected TBDs are first-class

A TBD may be:

```text
open + protected
open + unprotected
resolved
```

A protected open TBD means later generation/workflow systems must not resolve it silently.

P2 exposes the query; later phases enforce it.

## 2.10 Canon export is deterministic

Given the same database state, export must produce byte-for-byte identical Markdown except for intentionally included data.

Do not include “exported at” timestamps inside the file.

---

# 3. Domain Model Refinement

The master spec originally sketched one `CanonEntity` row containing `status`, `data`, and `revision`.

This plan deliberately refines that model into:

```text
CanonEntity
    ↓
CanonSection
    ↓
CanonSectionRevision
```

Reason:

> Section-level locking and section-level history are core product requirements. Storing an entire Story/Character document as one mutable JSON blob would create unnecessary revisions, conflict with independent lock semantics, and force later workflows to read unrelated data.

This is a compatible refinement of the master product design, not a scope expansion.

---

# 4. Canon Entity Types

Use exactly:

```text
story
character
location
faction
world_rule
production_rules
```

## Entity responsibilities

### `story`

One singleton per project.

Sections:

```text
premise
thesis
timeline
aesthetic
relationships
structural_engines
active_skill_rules
```

### `character`

Zero or more.

Sections:

```text
role_tag
visual_summary
function
backstory
psychology
speech
movement
stillness
visual_locks
sub_beats
```

### `location`

Zero or more.

Sections:

```text
description
visual_tags
geography
rules
```

### `faction`

Zero or more.

Sections:

```text
description
visual_signature
public_face
actual_behavior
```

### `world_rule`

Zero or more.

Sections:

```text
rule
notes
```

### `production_rules`

One singleton per project.

Sections:

```text
rules
```

Singleton rules are enforced in service code:
- one `story`;
- one `production_rules`.

---

# 5. Section Payload Types

All section values are JSON, but each entity type + section key has one exact schema.

Do not accept arbitrary JSON.

---

## 5.1 Common section wrapper

Database stores section metadata separately, so API DTO is:

```ts
export type CanonSectionStatus = "draft" | "locked";

export interface CanonSection<T = unknown> {
  id: string;
  entityId: string;
  key: string;
  value: T;
  status: CanonSectionStatus;
  revision: number;
  createdAt: string;
  updatedAt: string;
  lockedAt: string | null;
}
```

---

## 5.2 Story payloads

```ts
export interface PremiseValue {
  text: string;
}

export interface ThesisValue {
  text: string;
}

export interface TimelineEntry {
  id: string;
  label: string;
  description: string;
}

export interface TimelineValue {
  entries: TimelineEntry[];
}

export interface AestheticValue {
  visualRegister: string;
  palette: string[];
  materials: string[];
  lighting: string;
  atmosphere: string;
  exteriorPresence: string;
  anomalyRule: string;
  notes: string[];
}

export interface RelationshipsValue {
  text: string;
}

export interface StructuralEnginesValue {
  engines: Array<{
    id: string;
    title: string;
    description: string;
  }>;
}

export interface ActiveSkillRulesValue {
  text: string;
}
```

---

## 5.3 Character payloads

```ts
export interface RoleTagValue {
  text: string;
}

export interface VisualSummaryValue {
  text: string;
}

export interface FunctionValue {
  text: string;
}

export interface BackstoryValue {
  text: string;
}

export interface PsychologyValue {
  text: string;
}

export interface PromptReadyDescriptorValue {
  text: string;
}

export interface VisualLock {
  id: string;
  key: string;
  description: string;
  severity: "required" | "important";
  validatorHint: string | null;
}

export interface VisualLocksValue {
  locks: VisualLock[];
}

export interface CharacterSubBeat {
  id: string;
  title: string;
  text: string;
}

export interface SubBeatsValue {
  beats: CharacterSubBeat[];
}
```

`speech`, `movement`, and `stillness` use `PromptReadyDescriptorValue`.

Do not automatically add quotation marks to stored values. The exporter adds the required display quotes.

---

## 5.4 Location payloads

```ts
export interface LocationDescriptionValue {
  text: string;
}

export interface VisualTagsValue {
  tags: string[];
}

export interface GeographyValue {
  text: string;
}

export interface LocationRulesValue {
  rules: string[];
}
```

---

## 5.5 Faction payloads

```ts
export interface FactionTextValue {
  text: string;
}
```

Used for:
- description;
- visual_signature;
- public_face;
- actual_behavior.

---

## 5.6 World Rule payloads

```ts
export interface WorldRuleValue {
  text: string;
}

export interface WorldRuleNotesValue {
  text: string;
}
```

---

## 5.7 Production Rule payloads

```ts
export interface ProductionRule {
  id: string;
  title: string;
  body: string;
}

export interface ProductionRulesValue {
  rules: ProductionRule[];
}
```

---

# 6. Canon Entity DTOs

`packages/domain/src/canon.ts`:

```ts
export const CANON_ENTITY_TYPES = [
  "story",
  "character",
  "location",
  "faction",
  "world_rule",
  "production_rules",
] as const;

export type CanonEntityType =
  (typeof CANON_ENTITY_TYPES)[number];

export interface CanonEntity {
  id: string;
  projectId: string;
  type: CanonEntityType;
  name: string;
  slug: string;
  createdAt: string;
  updatedAt: string;
}

export interface CanonEntityDetail {
  entity: CanonEntity;
  sections: CanonSection[];
}

export interface CreateCanonEntityInput {
  projectRootPath: string;
  type: Exclude<
    CanonEntityType,
    "story" | "production_rules"
  >;
  name: string;
}

export interface UpsertCanonSectionInput<T = unknown> {
  projectRootPath: string;
  entityId: string;
  sectionKey: string;
  value: T;
  reason?: string | null;
}

export interface SetCanonSectionLockInput {
  projectRootPath: string;
  sectionId: string;
  reason?: string | null;
}
```

Singleton creation is separate:

```ts
export interface EnsureCanonSingletonsInput {
  projectRootPath: string;
}
```

---

# 7. Revision DTOs

```ts
export type CanonChangeKind =
  | "create"
  | "edit"
  | "lock"
  | "unlock";

export interface CanonSectionRevision {
  id: string;
  sectionId: string;
  revision: number;
  value: unknown;
  status: CanonSectionStatus;
  changeKind: CanonChangeKind;
  reason: string | null;
  createdAt: string;
}
```

No restore API in P2.

History is read-only.

A later plan may add “restore revision” as a new explicit mutation that itself creates a new revision.

---

# 8. TBD DTOs

```ts
export type CanonTbdStatus = "open" | "resolved";

export interface CanonTbd {
  id: string;
  projectId: string;
  canonEntityId: string | null;
  sectionKey: string | null;
  topic: string;
  note: string | null;
  protected: boolean;
  status: CanonTbdStatus;
  resolutionText: string | null;
  createdAt: string;
  updatedAt: string;
  resolvedAt: string | null;
}

export interface CreateCanonTbdInput {
  projectRootPath: string;
  canonEntityId?: string | null;
  sectionKey?: string | null;
  topic: string;
  note?: string | null;
  protected: boolean;
}

export interface ResolveCanonTbdInput {
  projectRootPath: string;
  tbdId: string;
  resolutionText: string;
}

export interface ReopenCanonTbdInput {
  projectRootPath: string;
  tbdId: string;
}
```

---

# 9. Database Migration

Create:

```text
apps/desktop/src-tauri/migrations/0003_canon_engine.sql
```

SQL:

```sql
CREATE TABLE canon_entities (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  type TEXT NOT NULL CHECK (
    type IN (
      'story',
      'character',
      'location',
      'faction',
      'world_rule',
      'production_rules'
    )
  ),
  name TEXT NOT NULL,
  slug TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,

  FOREIGN KEY (project_id)
    REFERENCES projects(id),

  UNIQUE(project_id, type, slug)
);

CREATE INDEX idx_canon_entities_project_type
  ON canon_entities(project_id, type);

CREATE TABLE canon_sections (
  id TEXT PRIMARY KEY,
  canon_entity_id TEXT NOT NULL,
  section_key TEXT NOT NULL,
  value_json TEXT NOT NULL,
  status TEXT NOT NULL CHECK (
    status IN ('draft', 'locked')
  ),
  revision INTEGER NOT NULL CHECK (revision > 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  locked_at TEXT,

  FOREIGN KEY (canon_entity_id)
    REFERENCES canon_entities(id),

  UNIQUE(canon_entity_id, section_key)
);

CREATE INDEX idx_canon_sections_entity
  ON canon_sections(canon_entity_id);

CREATE INDEX idx_canon_sections_status
  ON canon_sections(status);

CREATE TABLE canon_section_revisions (
  id TEXT PRIMARY KEY,
  canon_section_id TEXT NOT NULL,
  revision INTEGER NOT NULL CHECK (revision > 0),
  value_json TEXT NOT NULL,
  status TEXT NOT NULL CHECK (
    status IN ('draft', 'locked')
  ),
  change_kind TEXT NOT NULL CHECK (
    change_kind IN ('create', 'edit', 'lock', 'unlock')
  ),
  reason TEXT,
  created_at TEXT NOT NULL,

  FOREIGN KEY (canon_section_id)
    REFERENCES canon_sections(id),

  UNIQUE(canon_section_id, revision)
);

CREATE INDEX idx_canon_revisions_section
  ON canon_section_revisions(
    canon_section_id,
    revision DESC
  );

CREATE TABLE canon_tbds (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  canon_entity_id TEXT,
  section_key TEXT,
  topic TEXT NOT NULL,
  note TEXT,
  protected INTEGER NOT NULL CHECK (
    protected IN (0, 1)
  ),
  status TEXT NOT NULL CHECK (
    status IN ('open', 'resolved')
  ),
  resolution_text TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  resolved_at TEXT,

  FOREIGN KEY (project_id)
    REFERENCES projects(id),

  FOREIGN KEY (canon_entity_id)
    REFERENCES canon_entities(id)
);

CREATE INDEX idx_canon_tbds_project_status
  ON canon_tbds(project_id, status);

CREATE INDEX idx_canon_tbds_protected_open
  ON canon_tbds(project_id, protected, status);
```

Do not add FTS or vector tables in P2.

---

# 10. Slug Rules

Entity slug is stable after creation.

Algorithm:

1. Unicode normalize;
2. lowercase;
3. convert runs of non-alphanumeric characters to `-`;
4. trim `-`;
5. if empty, use `entity`;
6. if collision exists for same project/type, append:
   ```text
   -2
   -3
   ...
   ```

Examples:

```text
Mara Keene → mara-keene
The Station → the-station
The Station (duplicate) → the-station-2
```

Do not derive foreign keys from slug.

---

# 11. Section Schemas and Validation

Implement the canonical map in both TypeScript and Rust.

## TypeScript

Create:

```text
packages/domain/src/canon-schema.ts
```

Use Zod.

Example:

```ts
export const premiseSchema = z.object({
  text: z.string(),
});

export const visualLockSchema = z.object({
  id: z.string().min(1),
  key: z.string().min(1),
  description: z.string().min(1),
  severity: z.enum(["required", "important"]),
  validatorHint: z.string().nullable(),
});
```

Create:

```ts
export const CANON_SECTION_SCHEMAS = {
  story: {
    premise: premiseSchema,
    thesis: thesisSchema,
    timeline: timelineSchema,
    aesthetic: aestheticSchema,
    relationships: relationshipsSchema,
    structural_engines: structuralEnginesSchema,
    active_skill_rules: activeSkillRulesSchema,
  },
  ...
} as const;
```

## Rust

Create:

```text
apps/desktop/src-tauri/src/canon/schema.rs
```

Use typed structs + `serde`.

Validation function:

```rust
pub fn validate_section_value(
    entity_type: CanonEntityType,
    section_key: &str,
    value: &serde_json::Value,
) -> Result<(), AppError>;
```

Unknown section key must fail:

```text
UNKNOWN_CANON_SECTION
```

Invalid payload must fail:

```text
INVALID_CANON_SECTION_VALUE
```

This duplicated validation is intentional because:
- frontend validation improves UX;
- backend validation enforces persistence integrity.

Add contract fixture tests to prevent drift between TS and Rust.

---

# 12. Error Contract Additions

Extend Rust `AppError`:

```rust
#[error("Canon entity was not found")]
CanonEntityNotFound,

#[error("Canon section was not found")]
CanonSectionNotFound,

#[error("Canon entity name must contain 1 to 160 characters")]
InvalidCanonEntityName,

#[error("This canon entity type is reserved as a project singleton")]
CanonSingletonTypeRequired,

#[error("Unknown canon section for this entity type")]
UnknownCanonSection,

#[error("Canon section value does not match its schema")]
InvalidCanonSectionValue,

#[error("Locked canon sections must be unlocked before editing")]
CanonSectionLocked,

#[error("Canon section is already locked")]
CanonSectionAlreadyLocked,

#[error("Canon section is already unlocked")]
CanonSectionAlreadyUnlocked,

#[error("Canon TBD was not found")]
CanonTbdNotFound,

#[error("Canon TBD topic must contain 1 to 240 characters")]
InvalidCanonTbdTopic,

#[error("Resolution text must not be blank")]
InvalidCanonTbdResolution,

#[error("TBD references a canon entity from another project")]
CanonTbdEntityProjectMismatch,

#[error("TBD section key does not exist on the referenced canon entity")]
CanonTbdSectionMismatch,

#[error("Story Bible export failed: {0}")]
CanonExport(String),
```

Frontend codes use existing stable error serialization.

---

# 13. File Structure Additions

Add:

```text
packages/domain/src/
├── canon.ts
├── canon.test.ts
├── canon-schema.ts
├── canon-schema.test.ts
└── tbd.ts

apps/desktop/src-tauri/
├── migrations/
│   └── 0003_canon_engine.sql
└── src/
    └── canon/
        ├── mod.rs
        ├── model.rs
        ├── schema.rs
        ├── repository.rs
        ├── revisions.rs
        ├── service.rs
        ├── tbd.rs
        ├── export.rs
        └── commands.rs

apps/desktop/src/features/
└── canon/
    ├── api.ts
    ├── CanonWorkspace.tsx
    ├── CanonWorkspace.test.tsx
    ├── StoryCanonEditor.tsx
    ├── StoryCanonEditor.test.tsx
    ├── CanonSectionCard.tsx
    ├── CanonSectionCard.test.tsx
    ├── CanonHistoryDialog.tsx
    ├── CanonHistoryDialog.test.tsx
    ├── CharacterList.tsx
    ├── CharacterEditor.tsx
    ├── CharacterEditor.test.tsx
    ├── LocationList.tsx
    ├── LocationEditor.tsx
    ├── FactionList.tsx
    ├── FactionEditor.tsx
    ├── WorldRulesEditor.tsx
    ├── ProductionRulesEditor.tsx
    ├── TbdPanel.tsx
    ├── TbdPanel.test.tsx
    └── ExportStoryBibleButton.tsx
```

---

# 14. Backend API

Tauri commands must remain thin wrappers.

Exact command set:

```rust
ensure_canon_singletons

create_canon_entity
list_canon_entities
get_canon_entity

upsert_canon_section
lock_canon_section
unlock_canon_section
list_canon_section_revisions

create_canon_tbd
list_canon_tbds
resolve_canon_tbd
reopen_canon_tbd

export_story_bible
```

No generic `execute_canon_action` command.

Explicit commands make state transitions auditable and easier to test.

---

# 15. Frontend API

Create:

```ts
export function ensureCanonSingletons(
  projectRootPath: string,
): Promise<CanonSingletonResult>;

export function createCanonEntity(
  input: CreateCanonEntityInput,
): Promise<CanonEntity>;

export function listCanonEntities(
  projectRootPath: string,
  type?: CanonEntityType,
): Promise<CanonEntity[]>;

export function getCanonEntity(
  projectRootPath: string,
  entityId: string,
): Promise<CanonEntityDetail>;

export function upsertCanonSection(
  input: UpsertCanonSectionInput,
): Promise<CanonSection>;

export function lockCanonSection(
  input: SetCanonSectionLockInput,
): Promise<CanonSection>;

export function unlockCanonSection(
  input: SetCanonSectionLockInput,
): Promise<CanonSection>;

export function listCanonSectionRevisions(
  projectRootPath: string,
  sectionId: string,
): Promise<CanonSectionRevision[]>;

export function createCanonTbd(
  input: CreateCanonTbdInput,
): Promise<CanonTbd>;

export function listCanonTbds(
  projectRootPath: string,
): Promise<CanonTbd[]>;

export function resolveCanonTbd(
  input: ResolveCanonTbdInput,
): Promise<CanonTbd>;

export function reopenCanonTbd(
  input: ReopenCanonTbdInput,
): Promise<CanonTbd>;

export function exportStoryBible(
  projectRootPath: string,
): Promise<StoryBibleExportResult>;
```

---

# 16. Task Plan

## Task 1 — Add Canon domain schemas and database migration

**Files:**
- Create: `packages/domain/src/canon.ts`
- Create: `packages/domain/src/canon-schema.ts`
- Create: `packages/domain/src/canon.test.ts`
- Create: `packages/domain/src/canon-schema.test.ts`
- Create: `packages/domain/src/tbd.ts`
- Modify: `packages/domain/src/index.ts`
- Modify: `packages/domain/package.json`
- Create: `apps/desktop/src-tauri/migrations/0003_canon_engine.sql`
- Modify: `apps/desktop/src-tauri/src/db/migrations.rs`
- Modify: `apps/desktop/src-tauri/src/error.rs`
- Create: `apps/desktop/src-tauri/src/canon/mod.rs`
- Create: `apps/desktop/src-tauri/src/canon/model.rs`
- Create: `apps/desktop/src-tauri/src/canon/schema.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

**Produces:** typed Canon DTOs, Zod schemas, Rust schemas, migration 3.

- [ ] **Step 1: Write failing TypeScript schema tests**

Add Zod:

```bash
pnpm --filter @cinematic/domain add zod
```

Tests:

```ts
describe("canon schemas", () => {
  it("accepts a valid premise", () => {
    expect(
      premiseSchema.parse({
        text: "A lone operator receives her own future voice.",
      }),
    ).toEqual({
      text: "A lone operator receives her own future voice.",
    });
  });

  it("rejects an invalid visual lock severity", () => {
    expect(() =>
      visualLockSchema.parse({
        id: "scar",
        key: "right_eyebrow_scar",
        description: "Scar on character-right eyebrow",
        severity: "optional",
        validatorHint: null,
      }),
    ).toThrow();
  });

  it("rejects duplicate visual lock keys", () => {
    expect(() =>
      visualLocksSchema.parse({
        locks: [
          {
            id: "one",
            key: "scar",
            description: "A",
            severity: "required",
            validatorHint: null,
          },
          {
            id: "two",
            key: "scar",
            description: "B",
            severity: "important",
            validatorHint: null,
          },
        ],
      }),
    ).toThrow();
  });
});
```

Run:

```bash
pnpm --filter @cinematic/domain test
```

Expected: FAIL.

- [ ] **Step 2: Implement TypeScript canon types and schemas**

Implement all types from Sections 5–8.

Add custom Zod refinements:

- visual lock keys unique;
- timeline entry IDs unique;
- production rule IDs unique;
- structural engine IDs unique;
- sub-beat IDs unique;
- trimmed names/text where a value is required.

Do not enforce “must contain canon content” on optional story sections. Empty draft values are valid.

Run domain tests.

Expected: PASS.

- [ ] **Step 3: Write failing Rust schema tests**

Create examples matching TypeScript fixtures.

Tests:

```rust
#[test]
fn accepts_valid_character_visual_locks() {
    let value = serde_json::json!({
        "locks": [{
            "id": "scar",
            "key": "right_eyebrow_scar",
            "description": "Small healed scar on character-right eyebrow.",
            "severity": "required",
            "validatorHint": "Character-right appears viewer-left in frontal images."
        }]
    });

    validate_section_value(
        CanonEntityType::Character,
        "visual_locks",
        &value,
    ).unwrap();
}

#[test]
fn rejects_story_section_on_character() {
    let error = validate_section_value(
        CanonEntityType::Character,
        "premise",
        &serde_json::json!({"text": "x"}),
    ).unwrap_err();

    assert!(matches!(
        error,
        AppError::UnknownCanonSection
    ));
}
```

Run:

```bash
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml canon::schema
```

Expected: FAIL.

- [ ] **Step 4: Implement Rust canon schemas**

Create typed serde structs mirroring TS.

Implement `validate_section_value`.

Add duplicate-ID/key checks not naturally enforced by serde.

Run Rust schema tests.

Expected: PASS.

- [ ] **Step 5: Add migration 0003**

Use exact SQL from Section 9.

Register migration version `3`.

Write migration test:

```rust
#[test]
fn canon_migration_creates_required_tables() {
    ...
}
```

Assert these tables exist:

```text
canon_entities
canon_sections
canon_section_revisions
canon_tbds
```

Run:

```bash
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml db::migrations
```

Expected: PASS.

- [ ] **Step 6: Commit schema foundation**

```bash
git add packages/domain apps/desktop/src-tauri
git commit -m "feat: add canon engine schemas"
```

**Task 1 acceptance:** TS and Rust validate the same canonical payload vocabulary; migration 3 applies successfully to both new and Sprint 1 projects.

---

## Task 2 — Implement Canon entity creation, singleton bootstrap, and retrieval

**Files:**
- Create: `apps/desktop/src-tauri/src/canon/repository.rs`
- Create: `apps/desktop/src-tauri/src/canon/service.rs`
- Create: `apps/desktop/src-tauri/src/canon/commands.rs`
- Modify: `apps/desktop/src-tauri/src/canon/mod.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Test: Rust tests in `canon/service.rs`
- Create: `apps/desktop/src/features/canon/api.ts`

**Produces:** stable canon containers before section editing.

- [ ] **Step 1: Write failing singleton tests**

Tests:

```rust
#[test]
fn ensure_singletons_creates_one_story_and_one_production_rules_entity() {
    let fixture = ProjectFixture::new();

    let first = CanonService::ensure_singletons(
        &fixture.root,
    ).unwrap();

    let second = CanonService::ensure_singletons(
        &fixture.root,
    ).unwrap();

    assert_eq!(first.story.id, second.story.id);
    assert_eq!(
        first.production_rules.id,
        second.production_rules.id
    );

    let all = CanonService::list_entities(
        &fixture.root,
        None,
    ).unwrap();

    assert_eq!(
        all.iter().filter(|e| e.entity_type == "story").count(),
        1
    );

    assert_eq!(
        all.iter()
            .filter(|e| e.entity_type == "production_rules")
            .count(),
        1
    );
}
```

Run canon tests. Expected: FAIL.

- [ ] **Step 2: Implement slug generation and entity repository**

Repository:

```rust
pub fn insert_entity(
    tx: &rusqlite::Transaction<'_>,
    record: &CanonEntityRecord,
) -> Result<(), AppError>;

pub fn list_entities(
    conn: &rusqlite::Connection,
    project_id: &str,
    entity_type: Option<CanonEntityType>,
) -> Result<Vec<CanonEntityRecord>, AppError>;

pub fn get_entity(
    conn: &rusqlite::Connection,
    entity_id: &str,
) -> Result<CanonEntityRecord, AppError>;

pub fn find_singleton(
    conn: &rusqlite::Connection,
    project_id: &str,
    entity_type: CanonEntityType,
) -> Result<Option<CanonEntityRecord>, AppError>;

pub fn slug_exists(
    conn: &rusqlite::Connection,
    project_id: &str,
    entity_type: CanonEntityType,
    slug: &str,
) -> Result<bool, AppError>;
```

Service slug allocation must be deterministic.

- [ ] **Step 3: Implement singleton bootstrap**

Exact method:

```rust
pub fn ensure_singletons(
    project_root: &Path,
) -> Result<CanonSingletonsDto, AppError>;
```

Create:

```text
story
name = Story
slug = story

production_rules
name = Production Rules
slug = production-rules
```

Use one transaction.

Repeated calls are idempotent.

- [ ] **Step 4: Write failing entity creation tests**

Test:

```rust
#[test]
fn creates_two_character_entities_with_collision_safe_slugs() {
    let fixture = ProjectFixture::new();

    let first = CanonService::create_entity(
        &fixture.root,
        CanonEntityType::Character,
        "Mara Keene",
    ).unwrap();

    let second = CanonService::create_entity(
        &fixture.root,
        CanonEntityType::Character,
        "Mara Keene",
    ).unwrap();

    assert_eq!(first.slug, "mara-keene");
    assert_eq!(second.slug, "mara-keene-2");
}
```

Also test generic create rejects:
- story;
- production_rules.

Expected error:

```text
CanonSingletonTypeRequired
```

- [ ] **Step 5: Implement create/list/get service APIs**

Methods:

```rust
pub fn create_entity(
    project_root: &Path,
    entity_type: CanonEntityType,
    name: &str,
) -> Result<CanonEntityDto, AppError>;

pub fn list_entities(
    project_root: &Path,
    entity_type: Option<CanonEntityType>,
) -> Result<Vec<CanonEntityDto>, AppError>;

pub fn get_entity(
    project_root: &Path,
    entity_id: &str,
) -> Result<CanonEntityDetailDto, AppError>;
```

`get_entity` includes current sections ordered by the canonical section order for entity type.

- [ ] **Step 6: Expose commands and frontend wrappers**

Tauri:
- `ensure_canon_singletons`
- `create_canon_entity`
- `list_canon_entities`
- `get_canon_entity`

Add typed wrappers.

- [ ] **Step 7: Verify restart persistence**

Create:
- story singleton;
- Mara character;
- location.

Close DB/service scope.

Reopen project.

Assert same IDs/slugs.

- [ ] **Step 8: Commit entity substrate**

```bash
git add apps/desktop/src-tauri apps/desktop/src/features/canon
git commit -m "feat: add canon entity management"
```

**Task 2 acceptance:** projects get stable Story + Production Rules singletons; characters/locations/factions/world rules can be created and survive reopen.

---

## Task 3 — Implement section editing, locking, and append-only revision history

**Files:**
- Create: `apps/desktop/src-tauri/src/canon/revisions.rs`
- Modify: `apps/desktop/src-tauri/src/canon/repository.rs`
- Modify: `apps/desktop/src-tauri/src/canon/service.rs`
- Modify: `apps/desktop/src-tauri/src/canon/commands.rs`
- Modify: `apps/desktop/src/features/canon/api.ts`
- Test: Rust section/revision tests
- Modify: `packages/domain/src/canon.test.ts`

**Produces:** the core Canon state machine.

- [ ] **Step 1: Write failing revision tests**

Create Story singleton then:

```rust
#[test]
fn creating_section_creates_revision_one() {
    let fixture = CanonFixture::new();

    let section = CanonService::upsert_section(
        &fixture.root,
        &fixture.story_id,
        "premise",
        serde_json::json!({
            "text": "A lone radio operator receives her future voice."
        }),
        Some("Initial premise".to_string()),
    ).unwrap();

    assert_eq!(section.revision, 1);
    assert_eq!(section.status, "draft");

    let history =
        CanonService::list_section_revisions(
            &fixture.root,
            &section.id,
        ).unwrap();

    assert_eq!(history.len(), 1);
    assert_eq!(history[0].change_kind, "create");
}
```

Write tests for:
- edit → revision 2;
- lock → revision 3;
- locked edit fails;
- unlock → revision 4;
- edit after unlock → revision 5.

Run.

Expected: FAIL.

- [ ] **Step 2: Implement section repository primitives**

```rust
pub fn get_section_by_key(
    tx: &rusqlite::Transaction<'_>,
    entity_id: &str,
    section_key: &str,
) -> Result<Option<CanonSectionRecord>, AppError>;

pub fn get_section(
    conn: &rusqlite::Connection,
    section_id: &str,
) -> Result<CanonSectionRecord, AppError>;

pub fn insert_section(
    tx: &rusqlite::Transaction<'_>,
    record: &CanonSectionRecord,
) -> Result<(), AppError>;

pub fn update_section(
    tx: &rusqlite::Transaction<'_>,
    record: &CanonSectionRecord,
) -> Result<(), AppError>;

pub fn insert_revision(
    tx: &rusqlite::Transaction<'_>,
    revision: &CanonSectionRevisionRecord,
) -> Result<(), AppError>;

pub fn list_revisions(
    conn: &rusqlite::Connection,
    section_id: &str,
) -> Result<Vec<CanonSectionRevisionRecord>, AppError>;
```

History order:
- newest first in API/UI.

- [ ] **Step 3: Implement `upsert_section` transaction**

Signature:

```rust
pub fn upsert_section(
    project_root: &Path,
    entity_id: &str,
    section_key: &str,
    value: serde_json::Value,
    reason: Option<String>,
) -> Result<CanonSectionDto, AppError>;
```

Behavior if missing:
1. verify entity belongs to project;
2. validate section key + payload;
3. create section ULID;
4. status draft;
5. revision 1;
6. insert section;
7. insert revision 1 / `create`;
8. commit.

Behavior if existing:
1. reject if locked;
2. validate;
3. revision + 1;
4. update value;
5. insert revision / `edit`;
6. commit.

If revision insertion fails, section mutation rolls back.

- [ ] **Step 4: Implement lock transaction**

```rust
pub fn lock_section(
    project_root: &Path,
    section_id: &str,
    reason: Option<String>,
) -> Result<CanonSectionDto, AppError>;
```

Rules:
- section must exist;
- must be draft;
- revision + 1;
- status locked;
- `locked_at = now`;
- revision snapshot change_kind `lock`.

Locking does not require non-empty prose globally because some valid sections are empty lists.

Schema validity is already sufficient.

- [ ] **Step 5: Implement unlock transaction**

Same structure:
- must be locked;
- revision + 1;
- status draft;
- `locked_at = NULL`;
- change_kind `unlock`.

No mutation in same command.

User must unlock, then edit.

- [ ] **Step 6: Add revision-history integrity test**

Query direct DB after five transitions.

Assert:

```text
revisions = 1,2,3,4,5
```

and each revision snapshot preserves historical value/status.

Also assert current section revision equals max revision.

- [ ] **Step 7: Expose commands and wrappers**

Commands:
- `upsert_canon_section`
- `lock_canon_section`
- `unlock_canon_section`
- `list_canon_section_revisions`

- [ ] **Step 8: Commit Canon state machine**

```bash
git add apps/desktop/src-tauri packages/domain apps/desktop/src/features/canon
git commit -m "feat: add canon section locking and revisions"
```

**Task 3 acceptance:** premise can evolve draft → edit → locked → unlocked → edit with full append-only revision history and no mutation while locked.

---

## Task 4 — Build Story Canon workspace with section locking and history UI

**Files:**
- Create: `apps/desktop/src/features/canon/CanonWorkspace.tsx`
- Create: `apps/desktop/src/features/canon/CanonWorkspace.test.tsx`
- Create: `apps/desktop/src/features/canon/StoryCanonEditor.tsx`
- Create: `apps/desktop/src/features/canon/StoryCanonEditor.test.tsx`
- Create: `apps/desktop/src/features/canon/CanonSectionCard.tsx`
- Create: `apps/desktop/src/features/canon/CanonSectionCard.test.tsx`
- Create: `apps/desktop/src/features/canon/CanonHistoryDialog.tsx`
- Create: `apps/desktop/src/features/canon/CanonHistoryDialog.test.tsx`
- Modify: `apps/desktop/src/features/projects/ProjectWorkspace.tsx`
- Modify: `apps/desktop/src/styles/app.css`

**Produces:** usable Story Canon UI without AI.

- [ ] **Step 1: Write failing Story workspace tests**

Test sections appear:

```text
Premise
Thesis
Timeline
Aesthetic
Relationships
Structural Engines
Active Skill Rules
```

Test initial state:
- missing section renders Draft;
- Edit enabled;
- Lock disabled until current editor value validates.

Test locked state:
- value renders read-only;
- button `Unlock`;
- no editable textarea/field.

Run frontend tests.

Expected: FAIL.

- [ ] **Step 2: Implement CanonWorkspace navigation**

Add project workspace navigation:

```text
Assets
Canon
```

Canon view sub-navigation:

```text
Story
Characters
Locations
Factions
World Rules
Production Rules
TBDs
```

Only Story functionality is implemented in this task; other tabs may render `Coming in this plan` placeholders, not fake data.

- [ ] **Step 3: Implement generic CanonSectionCard**

Props:

```ts
interface CanonSectionCardProps<T> {
  title: string;
  section: CanonSection<T> | null;
  draftValue: T;
  validate: (value: unknown) => T;
  renderEditor: (...) => ReactNode;
  renderReadOnly: (...) => ReactNode;
  onSave: (...) => Promise<void>;
  onLock: (...) => Promise<void>;
  onUnlock: (...) => Promise<void>;
  onHistory: (...) => void;
}
```

Behavior:
- Save creates/edits draft.
- Locked sections are read-only.
- Lock and unlock are separate actions.
- Current revision is visible.
- Command errors are visible.
- No optimistic status transitions for lock/unlock.

- [ ] **Step 4: Implement Story section editors**

Use typed editors:

### Premise / Thesis / Relationships / Active Skill Rules
Textarea.

### Timeline
Rows:
- label;
- description;
- add/remove row.

Use client-generated ULID-equivalent? Do **not** introduce a second ID library if domain already has one. Use `crypto.randomUUID()` for list item IDs because these nested item IDs are document-local and not foreign keys.

### Aesthetic
Fields:
- visual register;
- palette tags;
- materials tags;
- lighting;
- atmosphere;
- exterior presence;
- anomaly rule;
- notes list.

### Structural Engines
Rows:
- title;
- description.

- [ ] **Step 5: Write failing history dialog test**

Given revisions 3,2,1:

UI shows:
- revision number;
- change kind;
- status;
- reason;
- timestamp;
- value snapshot.

Newest first.

No restore button in P2.

- [ ] **Step 6: Implement CanonHistoryDialog**

Fetch only when opened.

Display JSON-derived human-readable value, not raw unformatted JSON.

- [ ] **Step 7: Run frontend tests**

```bash
pnpm --filter @cinematic/desktop test
```

Expected: PASS.

- [ ] **Step 8: Manual smoke**

Create project:

```text
Red Door
```

Set:
- premise;
- thesis.

Lock premise.

Attempt to edit premise.

Expected:
- UI prevents edit;
- direct backend mutation test already guarantees server-side rejection.

Unlock premise and edit.

Confirm revision badge increments.

- [ ] **Step 9: Commit Story Canon UI**

```bash
git add apps/desktop/src/features/canon apps/desktop/src/features/projects
git commit -m "feat: add story canon editor"
```

**Task 4 acceptance:** a non-technical user can author, lock, unlock, and inspect history for all Story sections.

---

## Task 5 — Implement Character Canon including structured permanent visual locks

**Files:**
- Create: `apps/desktop/src/features/canon/CharacterList.tsx`
- Create: `apps/desktop/src/features/canon/CharacterEditor.tsx`
- Create: `apps/desktop/src/features/canon/CharacterEditor.test.tsx`
- Modify: `apps/desktop/src/features/canon/CanonWorkspace.tsx`
- Add Rust service tests for visual-lock query helper
- Modify: `apps/desktop/src-tauri/src/canon/service.rs`

**Produces:** Character narrative truth and machine-queryable visual locks.

- [ ] **Step 1: Write failing character creation/editor tests**

Test:
- create character `Mara Keene`;
- result gets slug `mara-keene`;
- character editor exposes sections:

```text
Role Tag
Visual Summary
Function
Backstory
Psychology
Speech
Movement
Stillness
Visual Locks
Sub-beats
```

- [ ] **Step 2: Implement CharacterList**

Controls:
- `New Character`;
- name input;
- create;
- list characters by name.

No delete in P2.

Selecting character loads `CharacterEditor`.

- [ ] **Step 3: Implement text section editors**

Use generic section card for:
- role tag;
- visual summary;
- function;
- backstory;
- psychology;
- speech;
- movement;
- stillness.

For Speech/Movement/Stillness display locked form with visible quote formatting:

```text
"descriptor text"
```

Stored value remains unquoted.

- [ ] **Step 4: Write visual-lock editor tests**

Test user can add:

```text
Key:
right_eyebrow_scar

Description:
Small healed linear scar through outer third of character-right eyebrow.

Severity:
required

Validator hint:
Character-right appears viewer-left in frontal images.
```

Test duplicate key fails client validation.

- [ ] **Step 5: Implement VisualLocksEditor**

Rows:
- key;
- description;
- severity;
- validator hint;
- remove.

Visual-lock list is stored as one structured `visual_locks` section so lock/edit/history semantics remain identical to other canon.

When section is locked:
- rows are read-only;
- no add/remove.

- [ ] **Step 6: Add backend query helper for later QA**

Do not add new Tauri command solely for future phases.

Add service method:

```rust
pub fn get_locked_character_visual_locks(
    project_root: &Path,
    character_entity_id: &str,
) -> Result<Vec<VisualLockDto>, AppError>;
```

Rules:
- if visual_locks section missing → empty list;
- if section draft → empty list;
- if locked → parsed locks.

This establishes the future QA boundary.

Test:
- draft locks not returned;
- locked locks returned.

- [ ] **Step 7: Implement SubBeats editor**

Rows:
- title;
- text.

Keep optional.

- [ ] **Step 8: Create Red Door character smoke data manually**

Via UI enter:

```text
Name:
Mara Keene

Role Tag:
The Verifier
```

Add required lock:
- `right_eyebrow_scar`.

Lock the visual-lock section.

Close/reopen.

Confirm same character/lock.

- [ ] **Step 9: Commit Character Canon**

```bash
git add apps/desktop/src/features/canon apps/desktop/src-tauri/src/canon
git commit -m "feat: add character canon and visual locks"
```

**Task 5 acceptance:** Character Canon is structured and revisioned; locked visual locks can be queried deterministically for future QA.

---

## Task 6 — Implement Locations, Factions, World Rules, and Production Rules

**Files:**
- Create: `apps/desktop/src/features/canon/LocationList.tsx`
- Create: `apps/desktop/src/features/canon/LocationEditor.tsx`
- Create: `apps/desktop/src/features/canon/FactionList.tsx`
- Create: `apps/desktop/src/features/canon/FactionEditor.tsx`
- Create: `apps/desktop/src/features/canon/WorldRulesEditor.tsx`
- Create: `apps/desktop/src/features/canon/ProductionRulesEditor.tsx`
- Add associated tests
- Modify: `apps/desktop/src/features/canon/CanonWorkspace.tsx`

**Produces:** remaining deterministic Story Bible canon categories.

- [ ] **Step 1: Write failing Location editor test**

Create `The Station`.

Sections:
- Description
- Visual Tags
- Geography
- Rules

Test locking Geography prevents edit independently of Description.

- [ ] **Step 2: Implement Locations**

Visual Tags:
- list of short strings.

Rules:
- list of strings.

No map/GIS in P2.

- [ ] **Step 3: Write and implement Faction editor**

Sections:
- Description
- Visual Signature
- Public Face
- Actual Behavior

Faction is optional but fully supported.

No relationships graph.

- [ ] **Step 4: Write and implement World Rules list**

Each World Rule is its own entity.

Create flow:
- name;
- section `rule`;
- optional section `notes`.

Example:

```text
Name:
Anomaly Uses Radio Infrastructure

Rule:
The anomaly manifests only through existing radio infrastructure.
```

This lets future workflows retrieve rules independently.

- [ ] **Step 5: Write and implement Production Rules singleton editor**

`ensure_canon_singletons` provides Production Rules entity.

Single `rules` section holds rows:
- title;
- body.

Example:

```text
Title:
Unknown canon stays unknown

Body:
Anything marked TBD must not be resolved by downstream generation.
```

- [ ] **Step 6: Add backend locked-section query helpers**

Add:

```rust
pub fn list_locked_world_rules(
    project_root: &Path,
) -> Result<Vec<LockedWorldRuleDto>, AppError>;

pub fn get_locked_production_rules(
    project_root: &Path,
) -> Result<Vec<ProductionRuleDto>, AppError>;
```

No Tauri commands needed solely for future work unless the UI uses them.

Tests:
- drafts excluded;
- locked included.

- [ ] **Step 7: Run full frontend and Rust tests**

```bash
pnpm test
pnpm test:rust
```

Expected: PASS.

- [ ] **Step 8: Commit supporting canon categories**

```bash
git add apps/desktop/src/features/canon apps/desktop/src-tauri/src/canon
git commit -m "feat: add world and production canon editors"
```

**Task 6 acceptance:** location geography, factions, world rules, and production rules have the same lock/history semantics as Story and Character Canon.

---

## Task 7 — Implement protected TBD state and project-wide TBD UI

**Files:**
- Create: `apps/desktop/src-tauri/src/canon/tbd.rs`
- Modify: `apps/desktop/src-tauri/src/canon/repository.rs`
- Modify: `apps/desktop/src-tauri/src/canon/service.rs`
- Modify: `apps/desktop/src-tauri/src/canon/commands.rs`
- Modify: `apps/desktop/src/features/canon/api.ts`
- Create: `apps/desktop/src/features/canon/TbdPanel.tsx`
- Create: `apps/desktop/src/features/canon/TbdPanel.test.tsx`
- Modify: `apps/desktop/src/features/canon/CanonWorkspace.tsx`
- Test: Rust TBD tests

**Produces:** future workflow firewall data.

- [ ] **Step 1: Write failing TBD service tests**

Tests:

```rust
#[test]
fn creates_project_level_protected_tbd() {
    let fixture = CanonFixture::new();

    let tbd = CanonService::create_tbd(
        &fixture.root,
        None,
        None,
        "What is behind the red door?",
        Some("Do not visualize before reveal.".to_string()),
        true,
    ).unwrap();

    assert_eq!(tbd.status, "open");
    assert!(tbd.protected);
}

#[test]
fn creates_section_scoped_tbd() {
    ...
}

#[test]
fn rejects_entity_from_another_project() {
    ...
}
```

Run.

Expected: FAIL.

- [ ] **Step 2: Implement TBD repository**

Functions:

```rust
pub fn insert_tbd(
    conn: &rusqlite::Connection,
    record: &CanonTbdRecord,
) -> Result<(), AppError>;

pub fn list_tbds(
    conn: &rusqlite::Connection,
    project_id: &str,
) -> Result<Vec<CanonTbdRecord>, AppError>;

pub fn get_tbd(
    conn: &rusqlite::Connection,
    tbd_id: &str,
) -> Result<CanonTbdRecord, AppError>;

pub fn update_tbd(
    conn: &rusqlite::Connection,
    record: &CanonTbdRecord,
) -> Result<(), AppError>;
```

Order:
1. open protected;
2. open unprotected;
3. resolved;
4. then created time ascending within group.

- [ ] **Step 3: Implement TBD creation validation**

If entity supplied:
- it must belong to project.

If section key supplied:
- entity must be supplied;
- section must currently exist on that entity.

Topic:
- trim;
- length 1–240.

- [ ] **Step 4: Implement resolve**

```rust
pub fn resolve_tbd(
    project_root: &Path,
    tbd_id: &str,
    resolution_text: &str,
) -> Result<CanonTbdDto, AppError>;
```

Rules:
- resolution must not be blank;
- status → resolved;
- set resolution_text;
- set resolved_at and updated_at.

Do **not** automatically mutate referenced canon section.

Resolution documents the decision; the user separately edits canon.

This avoids hidden cross-entity mutation.

- [ ] **Step 5: Implement reopen**

Rules:
- status → open;
- resolution_text → null;
- resolved_at → null.

Preserve protected flag.

- [ ] **Step 6: Add future-firewall helper**

```rust
pub fn list_open_protected_tbds(
    project_root: &Path,
) -> Result<Vec<CanonTbdDto>, AppError>;
```

Test only open + protected are returned.

No workflow enforcement yet.

- [ ] **Step 7: Expose Tauri commands and frontend wrappers**

Commands:
- `create_canon_tbd`
- `list_canon_tbds`
- `resolve_canon_tbd`
- `reopen_canon_tbd`

- [ ] **Step 8: Write failing TBD UI tests**

UI must visibly distinguish:

```text
PROTECTED
OPEN
RESOLVED
```

Test resolve requires non-empty resolution.

- [ ] **Step 9: Implement TbdPanel**

Create form:
- topic;
- note;
- protected toggle;
- optional entity scope;
- optional existing section scope.

List cards show:
- topic;
- scope;
- note;
- status;
- protected badge;
- resolution if resolved;
- Resolve/Reopen action.

- [ ] **Step 10: Add Red Door smoke TBD**

Create:

```text
Topic:
What is behind the red door?

Protected:
true

Note:
No world plate or generation may reveal the space before canon intentionally resolves it.
```

Close/reopen.

Confirm state.

- [ ] **Step 11: Commit TBD firewall state**

```bash
git add apps/desktop/src-tauri/src/canon apps/desktop/src/features/canon
git commit -m "feat: add protected canon TBDs"
```

**Task 7 acceptance:** protected story unknowns are explicit, persistent, queryable, resolvable, and reopenable without silently mutating canon.

---

## Task 8 — Implement deterministic Story Bible Markdown export

**Files:**
- Create: `apps/desktop/src-tauri/src/canon/export.rs`
- Modify: `apps/desktop/src-tauri/src/canon/service.rs`
- Modify: `apps/desktop/src-tauri/src/canon/commands.rs`
- Modify: `apps/desktop/src/features/canon/api.ts`
- Create: `apps/desktop/src/features/canon/ExportStoryBibleButton.tsx`
- Add Rust export snapshot tests
- Add frontend button test

**Produces:** `canon/story-bible.md`.

- [ ] **Step 1: Write failing deterministic export test**

Create fixture:
- premise locked;
- thesis draft;
- Mara character with locked role/function;
- one locked location geography;
- one locked world rule;
- one protected TBD;
- one locked production rule.

Call export twice.

Assert bytes equal.

- [ ] **Step 2: Lock export section ordering**

Exact top-level order:

```text
# <Project Name> — Story Bible

## 1. Premise
## 2. Thesis
## 3. World / Timeline
## 4. Aesthetic
## 5. Factions
## 6. Locations
## 7. World Rules
## 8. Characters
## 9. Relationships and Ensemble Dynamics
## 10. Structural Engines
## 11. Production Rules
## 12. When This Canon Is Active
## Open TBDs
```

Sections that do not exist render:

```text
[TBD]
```

Do not omit them because stable shape improves downstream readability.

- [ ] **Step 3: Define draft/locked markers**

Each section heading body begins with:

```text
**Status:** LOCKED
```

or:

```text
**Status:** DRAFT
```

Singleton or entity missing:

```text
[TBD]
```

For Character subsections, same marker is not repeated line-by-line in final prose. Instead use:

```text
#### Speech — LOCKED
```

This keeps export readable.

- [ ] **Step 4: Implement character formatting**

Exact shape:

```markdown
### MARA KEENE — *The Verifier*

**Visual:** ...

**Function in the story:** ...

**Backstory:** ...

**Present-tense psychology:** ...

**Speech:** "..."

**Movement:** "..."

**Stillness:** "..."

**Permanent visual locks:**
- [REQUIRED] right_eyebrow_scar — ...

**Sub-beats:**
- **The log:** ...
```

If a field is missing:

```text
[TBD]
```

Do not invent content.

- [ ] **Step 5: Implement TBD export**

Open TBDs only.

Format:

```markdown
## Open TBDs

- **[PROTECTED] What is behind the red door?**
  Scope: Location — The Station / Geography
  Note: ...
```

Resolved TBDs do not appear in the default Story Bible.

- [ ] **Step 6: Implement atomic file write**

Output:

```text
<project-root>/canon/story-bible.md
```

Process:
1. render entire string in memory;
2. write `story-bible.md.tmp`;
3. fsync;
4. rename atomically.

No timestamp in body.

Return:

```ts
export interface StoryBibleExportResult {
  relativePath: string;
  byteSize: number;
}
```

- [ ] **Step 7: Add snapshot test**

Store expected Markdown fixture in test source or inline string.

Assert exact equality.

This test is intentionally strict because export format is a public artifact contract.

- [ ] **Step 8: Add Export button**

Button:

```text
Export Story Bible
```

On success:
- show relative path;
- do not automatically open OS file manager in P2.

- [ ] **Step 9: Commit export**

```bash
git add apps/desktop/src-tauri/src/canon apps/desktop/src/features/canon
git commit -m "feat: export structured canon as story bible"
```

**Task 8 acceptance:** same DB state yields identical Markdown; output contains draft/locked state, visual locks, and protected open TBDs without inventing missing canon.

---

## Task 9 — End-to-end Canon Engine acceptance and restart verification

**Files:**
- Create: `apps/desktop/src-tauri/tests/canon_engine_acceptance.rs`
- Create: `docs/superpowers/plans/canon-engine-verification.md`
- Modify: `README.md`

**Produces:** proof that P2 works independently and is safe for Skill Runtime planning.

- [ ] **Step 1: Write the full acceptance test**

Scenario:

1. Create project `Red Door`.
2. Ensure Story + Production Rules singletons.
3. Set Premise.
4. Lock Premise.
5. Set Thesis but leave Draft.
6. Create Character `Mara Keene`.
7. Set Role Tag `The Verifier`; lock.
8. Set Function; lock.
9. Add Visual Locks:
   - `right_eyebrow_scar`
   - `no_bangs`
10. Lock Visual Locks.
11. Create Location `The Station`.
12. Set Geography; lock.
13. Create World Rule:
    - anomaly uses radio infrastructure;
    - lock Rule.
14. Add Production Rule:
    - unknown canon stays unknown;
    - lock.
15. Create protected TBD:
    - what is behind the red door?
16. Export Story Bible.
17. Drop all service/DB scopes.
18. Reopen project.
19. Verify all IDs/revisions/statuses.
20. Export again.
21. Verify export bytes equal.

- [ ] **Step 2: Assert locked edit protection after reopen**

After reopen:

Attempt direct service edit of locked Premise.

Expected:

```text
CanonSectionLocked
```

Then:
- unlock;
- edit;
- revision increases.

- [ ] **Step 3: Assert revision history survives reopen**

Premise history must show:

```text
create
lock
unlock
edit
```

with contiguous revisions.

- [ ] **Step 4: Assert future query boundaries**

Call:

```rust
get_locked_character_visual_locks(...)
list_locked_world_rules(...)
get_locked_production_rules(...)
list_open_protected_tbds(...)
```

Expected:
- only locked canon returned;
- draft thesis excluded;
- protected TBD returned.

This proves P2 exposes the correct future Skill Runtime inputs without implementing Skill Runtime itself.

- [ ] **Step 5: Create manual desktop verification guide**

Create:

```text
docs/superpowers/plans/canon-engine-verification.md
```

Checklist:

```markdown
# Canon Engine Desktop Verification

1. Open Sprint 1 project or create `Red Door`.
2. Open Canon → Story.
3. Enter Premise.
4. Save.
5. Lock Premise.
6. Confirm editor becomes read-only.
7. Open History and confirm Create + Lock revisions.
8. Unlock Premise.
9. Edit one sentence.
10. Save and confirm revision increments.
11. Create Character `Mara Keene`.
12. Set Role Tag `The Verifier`.
13. Add a required visual lock `right_eyebrow_scar`.
14. Lock Visual Locks.
15. Create Location `The Station`.
16. Enter and lock Geography.
17. Create a World Rule and lock it.
18. Add one Production Rule and lock it.
19. Add protected TBD `What is behind the red door?`.
20. Export Story Bible.
21. Open exported Markdown and verify all content.
22. Fully close app.
23. Reopen project.
24. Verify every canon section status and revision persists.
25. Verify the protected TBD remains open.
26. Export again.
27. Confirm exported Story Bible content is unchanged.
```

- [ ] **Step 6: Update README**

Add Canon Engine architecture notes:

```text
Locked sections = canonical.
Draft sections = working state.
Markdown = export, not source of truth.
Protected TBDs = future workflow firewall.
```

Document tests.

- [ ] **Step 7: Run full verification**

```bash
pnpm install
pnpm test
pnpm test:rust
pnpm --filter @cinematic/desktop tauri build --debug
```

Then:

```bash
pnpm dev
```

Complete manual checklist.

- [ ] **Step 8: Commit P2 acceptance**

```bash
git add README.md apps/desktop/src-tauri/tests docs/superpowers/plans
git commit -m "test: verify canon engine persistence"
```

**Task 9 acceptance:** structured Canon survives restart, locked state is enforced server-side, revisions are append-only, protected TBDs are queryable, and Markdown export is deterministic.

---

# 17. Canon Engine Acceptance Matrix

| Requirement | Test |
|---|---|
| Story Canon structured | Story editor + acceptance |
| Character Canon structured | Character editor + acceptance |
| Locations | Task 6 |
| Factions | Task 6 |
| World Rules | Task 6 |
| Production Rules | Task 6 |
| Section lock/unlock | Task 3 |
| Locked section cannot mutate | Task 3 + Task 9 |
| Revision history | Task 3 + Task 9 |
| Permanent visual locks | Task 5 |
| QA-queryable visual locks | Task 5 + Task 9 |
| Protected TBD | Task 7 |
| Open protected query | Task 7 + Task 9 |
| Restart persistence | Task 9 |
| Markdown export | Task 8 |
| Deterministic export | Task 8 + Task 9 |
| No AI/provider dependency | architecture inspection |

---

# 18. Cross-Task Invariants

Every task must preserve:

- [ ] SQLite remains the machine source of truth.
- [ ] Markdown never becomes mutable canonical storage.
- [ ] Canon entity IDs are immutable ULIDs.
- [ ] Slugs are display/navigation metadata, never foreign keys.
- [ ] Section status is only `draft` or `locked`.
- [ ] Locked section cannot be edited.
- [ ] Unlock is explicit.
- [ ] Every successful create/edit/lock/unlock creates exactly one revision.
- [ ] Revision history is append-only.
- [ ] Section value must validate against entity-type + section-key schema.
- [ ] Unknown section keys are rejected.
- [ ] Visual locks have unique keys inside a Character visual-lock section.
- [ ] Only locked visual locks are exposed to future QA query.
- [ ] Protected open TBDs are queryable project-wide.
- [ ] Resolving a TBD never silently edits canon.
- [ ] P2 does not call any AI model.
- [ ] P2 does not know provider/model concepts.
- [ ] Asset canonical state from Sprint 1 remains untouched.
- [ ] Existing project files and media are never overwritten by Canon operations.
- [ ] Canon export uses atomic temp-file rename.

---

# 19. P2 Definition of Done

P2 is complete only when:

1. A project automatically has stable Story and Production Rules canon containers.
2. A user can create Character, Location, Faction, and World Rule entities.
3. Story sections are independently editable.
4. Story sections are independently lockable.
5. Locked sections reject backend mutations.
6. Unlock is explicit and revisioned.
7. Every create/edit/lock/unlock has append-only history.
8. Character Speech/Movement/Stillness are stored as prompt-ready structured fields.
9. Permanent Character visual locks are structured, not only prose.
10. Locked visual locks are queryable for future QA.
11. Location geography can be locked independently.
12. World Rules can be authored and locked individually.
13. Production Rules can be authored and locked.
14. Protected TBDs can be created.
15. Protected TBDs can be scoped to project/entity/section.
16. TBDs can be resolved without silently mutating canon.
17. TBDs can be reopened.
18. Open protected TBDs are queryable.
19. Story Bible Markdown export is deterministic.
20. Export represents missing canon as `[TBD]` rather than inventing data.
21. Canon state survives complete application restart.
22. Section history survives restart.
23. Existing Sprint 1 asset state remains unaffected.
24. TypeScript tests pass.
25. Rust tests pass.
26. Canon acceptance test passes.
27. Tauri debug build succeeds.
28. Manual desktop verification passes.

---

# 20. What Must Not Leak Into P2

Reject implementation attempts that introduce any of these:

```text
OpenAI client
Gemini client
Ollama client
ComfyUI client
prompt generation
embeddings
vector DB
SkillDefinition
workflow execution
provider capability router
asset QA
face comparison
image generation
video generation
scene generation
Cinema Director
automatic Story Bible interview
```

If an implementation task appears to require one of these, stop and reassess. Canon Engine must remain deterministic and model-agnostic.

---

# 21. Recommended Commit Sequence

Expected commits:

```text
feat: add canon engine schemas
feat: add canon entity management
feat: add canon section locking and revisions
feat: add story canon editor
feat: add character canon and visual locks
feat: add world and production canon editors
feat: add protected canon TBDs
feat: export structured canon as story bible
test: verify canon engine persistence
```

Keep commits narrow enough to review independently.

---

# 22. Self-Review

## 22.1 Spec coverage

Master P2 requirements mapped:

- structured Story Canon → Tasks 1–4.
- Character Canon → Task 5.
- visual locks → Task 5.
- Locations → Task 6.
- Factions → Task 6.
- World Rules → Task 6.
- Production Rules → Task 6.
- lock/unlock → Task 3.
- revision history → Task 3.
- TBD entries → Task 7.
- protected TBD firewall query → Task 7.
- Markdown export → Task 8.
- restart persistence → Task 9.
- no AI dependency → global constraints + Task 9 inspection.

No P2 requirement is deferred.

## 22.2 Placeholder scan

Implementation steps contain:
- exact files;
- exact schemas;
- exact transitions;
- explicit error cases;
- test commands;
- acceptance conditions.

No vague “add validation”, “write tests”, or “similar to previous task” instructions remain.

## 22.3 Type/signature consistency

Checked:
- `CanonEntity` identifiers and types are consistent.
- Singleton entity types are consistently `story` and `production_rules`.
- Section state is consistently `draft | locked`.
- Revision change kinds are consistently `create | edit | lock | unlock`.
- Visual locks are stored in Character `visual_locks`.
- TBD status is consistently `open | resolved`.
- Story Bible export path is consistently `canon/story-bible.md`.
- Frontend command wrapper names correspond to explicit Tauri command names.
- Later query helpers return locked canon only.

No signature conflict remains.

---

# 23. Execution Handoff

When implementing this plan:

1. Work in an isolated git worktree.
2. Use `superpowers:subagent-driven-development` if available.
3. Give one task to one fresh implementation subagent.
4. Run the task's specified tests before review.
5. Review code against both:
   - task acceptance;
   - Cross-Task Invariants.
6. Commit before moving to next task.
7. Never begin the next architectural phase while P2 acceptance is failing.

After P2 passes completely:

> **Stop.**

The next plan should be **P3 — Skill / Workflow Runtime**.

Do not implement Skill Runtime opportunistically inside Canon Engine.

The P3 planning session must consume the *actual implemented* Canon Engine interfaces, especially:

```text
locked section query
locked visual-lock query
locked world-rule query
locked production-rule query
open protected TBD query
canon revision identifiers
```

Those become the deterministic context source for executable skills.
