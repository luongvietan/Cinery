# AI Cinematic Production OS — Master Implementation Plan

> **Purpose:** This document is intentionally self-contained. A capable AI coding agent should be able to understand the product, architecture, domain model, implementation order, constraints, and acceptance criteria without access to the conversation that produced it.

---

## 0. Executive Summary

Build a **local-first desktop application for AI filmmaking** that manages story canon, character identity, cinematic assets, AI-generation workflows, model/provider routing, visual QA, versioning, and prompt compilation.

The application is **not** a Higgsfield clone, prompt library, or ChatGPT-style chat wrapper.

The product thesis is:

> **Build canonical worlds once, then generate characters, environments, scenes, and shots across local or cloud AI models without losing consistency.**

The durable value is not generation itself. Models and providers change quickly. The durable layer is:

1. **Canon**
2. **Asset state**
3. **Executable skills/workflows**
4. **Versioning and provenance**
5. **Automatic QA**
6. **Repair instead of blind regeneration**
7. **Provider portability**

The initial product should be a **desktop-first hybrid**:
- local project files;
- local database;
- local AI where practical;
- cloud generation as optional adapters;
- no mandatory cloud account for MVP.

The canonical first workflow is:

```text
Create Project
    ↓
Build Story Canon
    ↓
Create Character
    ↓
Generate Face Lock
    ↓
QA / Repair
    ↓
Approve Canonical Face
    ↓
Create Outfit
    ↓
QA / Repair
    ↓
Create Character Sheet
    ↓
Create World Plate
    ↓
Create Scene
    ↓
Compile Video Prompt
```

If the app can execute this workflow reliably, with traceable state and no canon drift, the MVP is successful.

---

# 1. Product Definition

## 1.1 Working product category

**AI Cinematic Production OS**

Alternative positioning:
- local-first AI filmmaking workspace;
- production environment for canonical AI worlds;
- model-agnostic cinematic asset pipeline.

Avoid positioning as:
- “AI video generator”;
- “prompt manager”;
- “Higgsfield alternative”;
- “ChatGPT for filmmaking”;
- “ComfyUI replacement”.

Those definitions are too narrow and create the wrong architecture.

---

## 1.2 Core user problem

AI filmmakers currently jump between:
- LLM chat;
- image generators;
- video generators;
- reference-image folders;
- notes;
- prompts;
- editing tools;
- spreadsheets;
- project-management tools.

They manually remember:
- which face is canonical;
- which outfit belongs to which scene;
- which generated image is approved;
- which scar/piercing/hair state is current;
- which environment plate is canonical;
- which model generated an asset;
- which prompt generated it;
- which version is superseded;
- what must never drift;
- whether a new generation accidentally invents canon.

The result is:
- identity drift;
- wardrobe drift;
- world-layout drift;
- duplicated prompt work;
- wasted generation credits;
- non-reproducible outputs;
- chat-context exhaustion;
- accidental canon changes.

The application should make these states explicit and executable.

---

# 2. Design Principles

These are hard architecture rules.

## 2.1 Canon is data, not chat history

Chat may help edit canon, but canonical state must live in structured project data.

## 2.2 Newest is not canonical

A newly generated asset remains a candidate until explicitly approved.

## 2.3 Skills do not own providers

A workflow describes what needs to happen. A provider adapter determines where it runs.

```text
SKILL ≠ MODEL ≠ PROVIDER
```

## 2.4 References carry existing visual truth

Once a strong canonical reference exists, downstream prompts should not redundantly re-describe the entire face/outfit/world.

## 2.5 Unknown canon stays unknown

Generation must never silently resolve story questions marked `TBD`.

## 2.6 Repair before regenerate

When one feature fails, generate a minimal repair instruction instead of rebuilding the whole asset whenever possible.

## 2.7 Local-first, cloud-optional

Projects remain usable without cloud storage or cloud accounts.

## 2.8 Provenance is first-class

Every generated asset must be traceable to:
- input canon;
- input assets;
- skill version;
- provider;
- model;
- prompt;
- parameters;
- timestamp;
- parent asset/version.

## 2.9 Desktop UI is not chat-first

Chat/AI Director is a command surface. The main product is a production workspace.

## 2.10 YAGNI for MVP

Do not build:
- NLE/timeline editor;
- team collaboration;
- billing;
- cloud sync;
- marketplace;
- mobile client;
- model training;
- a custom diffusion runtime;
- a ComfyUI replacement.

---

# 3. Source Workflow Concepts to Preserve

The initial workflow is inspired by four production responsibilities. Reimplement the concepts in original application code and schemas; do not assume third-party skill files are redistributable.

## 3.1 Narrative Canon

Responsibilities:
- premise;
- thesis;
- world/timeline;
- aesthetic rules;
- locations;
- world rules;
- character function;
- psychology;
- speech;
- movement;
- stillness;
- narrative production rules.

Question answered:

> Who is this person, what world are they in, and how do they behave?

---

## 3.2 Character Visual Canon

Responsibilities:
- face identity;
- detailed visual traits;
- permanent additions;
- outfits;
- outfit replacement;
- character sheets;
- visual versioning.

Question answered:

> What exactly does this character look like?

---

## 3.3 Scene / World Visual Canon

Responsibilities:
- world plates;
- environment plates;
- character-in-environment stills;
- shot keyframes;
- scene stills.

Question answered:

> What does this place/frame look like?

---

## 3.4 Temporal / Cinema Compilation

Responsibilities:
- runtime;
- shot structure;
- camera;
- motion;
- continuity;
- performance;
- audio instructions;
- last-frame definition;
- provider-neutral video prompt compilation.

Question answered:

> How does canonical state move through time?

---

# 4. Product Architecture

## 4.1 High-level architecture

```text
┌─────────────────────────────────────────────┐
│                DESKTOP UI                   │
│ React + TypeScript                          │
├─────────────────────────────────────────────┤
│             PRODUCTION ENGINE               │
│                                             │
│ Project Kernel                              │
│ Canon Engine                                │
│ Asset Manifest                              │
│ Workflow / Skill Runtime                    │
│ Production Router                           │
│ Prompt Compiler                             │
│ QA Engine                                   │
│ Provider Router                             │
├───────────────────┬─────────────────────────┤
│ LOCAL AI          │ CLOUD PROVIDERS         │
│                   │                         │
│ local LLM         │ image generation        │
│ local VLM         │ image editing           │
│ embeddings        │ video generation        │
│ ComfyUI           │ voice / lipsync later   │
│ upscaling         │                         │
└───────────────────┴─────────────────────────┘
                     │
                     ▼
              LOCAL PROJECT FILES
```

---

# 5. Recommended Technology Stack

## 5.1 Desktop shell

**Tauri 2**

Reasons:
- small binary/runtime footprint;
- native filesystem access;
- good desktop security boundary;
- supports sidecars/process invocation;
- suitable for local AI workers;
- frontend remains web technology.

Do not use Electron unless a blocker is discovered during implementation.

---

## 5.2 Frontend

- React
- TypeScript
- Vite
- Tailwind CSS
- shadcn/ui or equivalent component primitives
- TanStack Query
- Zustand for lightweight local UI state
- React Router or TanStack Router

Avoid Next.js for the desktop shell unless there is a concrete requirement. This is a desktop product, not an SSR web application.

---

## 5.3 Persistence

**SQLite** for metadata.

Use:
- migrations;
- foreign keys;
- WAL mode;
- explicit repository/domain layer.

Actual media stays on disk, not in SQLite blobs.

---

## 5.4 Local files

Each project has its own directory.

Example:

```text
red-door/
├── project.yaml
├── project.db
├── canon/
│   ├── story-bible.md
│   └── canon.json
├── characters/
│   └── mara/
│       ├── identity/
│       ├── looks/
│       └── sheets/
├── worlds/
│   └── station/
├── props/
├── scenes/
│   └── scene-001/
│       └── shots/
├── prompts/
├── generations/
├── thumbnails/
└── exports/
```

`project.db` is the machine state.
Human-readable exports such as `canon.json` and Markdown are generated views/backups, not duplicate mutable sources of truth.

---

## 5.5 AI worker

Python service/sidecar.

Recommended:
- Python 3.12+
- FastAPI only if HTTP boundary is useful;
- otherwise JSON-RPC/stdin sidecar is acceptable;
- Pydantic;
- Pillow;
- OpenCV only where needed;
- provider SDKs behind adapters.

Responsibilities:
- local VLM;
- image comparison;
- embeddings;
- ComfyUI communication;
- image metadata;
- provider calls that are easier in Python.

Keep business-domain logic in the TypeScript/domain layer where possible. Python should be execution infrastructure, not the only source of product rules.

---

# 6. Monorepo Structure

Recommended:

```text
apps/
  desktop/
    src/
    src-tauri/

services/
  ai-worker/

packages/
  domain/
  database/
  project-kernel/
  canon/
  assets/
  workflows/
  skills/
  prompt-compiler/
  providers/
  qa/
  shared/
```

Purpose:

### `domain`
Pure types, entities, value objects, state transitions.

### `database`
SQLite schema, migrations, repositories.

### `project-kernel`
Create/open/validate project, paths, configuration.

### `canon`
Narrative canon schemas and mutation APIs.

### `assets`
Asset manifest, versions, dependencies, provenance.

### `workflows`
State-machine orchestration.

### `skills`
Skill definition format, parser/registry/runtime.

### `prompt-compiler`
Provider-neutral production instructions → provider prompt.

### `providers`
Image/video/local adapters.

### `qa`
Visual checks, rules, comparison, repair generation.

---

# 7. Core Domain Model

## 7.1 Project

```ts
interface Project {
  id: string;
  name: string;
  slug: string;
  createdAt: string;
  updatedAt: string;
  schemaVersion: number;
}
```

---

## 7.2 Canon Entity

```ts
type CanonEntityType =
  | "story"
  | "character"
  | "location"
  | "world_rule"
  | "aesthetic"
  | "prop"
  | "scene";

interface CanonEntity {
  id: string;
  projectId: string;
  type: CanonEntityType;
  name: string;
  status: "draft" | "locked";
  data: unknown;
  revision: number;
}
```

Use typed schemas by entity type instead of leaving production code on `unknown`.

---

## 7.3 Character Canon

Example:

```ts
interface CharacterCanon {
  id: string;
  storyName: string;
  roleTag?: string;

  narrative: {
    function?: string;
    backstory?: string;
    psychology?: string;
    speech?: string;
    movement?: string;
    stillness?: string;
  };

  permanentVisualLocks: VisualLock[];

  tbd: CanonTBD[];

  currentCanonicalFaceAssetId?: string;
}
```

---

## 7.4 Visual Lock

```ts
interface VisualLock {
  id: string;
  key: string;
  description: string;
  severity: "required" | "important";
  validatorHint?: string;
}
```

Examples:
- `right_eyebrow_scar`
- `warm_light_medium_skin`
- `no_bangs`
- `black_watch_left_wrist`

---

## 7.5 Asset

```ts
type AssetType =
  | "face_lock"
  | "outfit"
  | "character_sheet"
  | "world_plate"
  | "shot_keyframe"
  | "prop_plate"
  | "image"
  | "video"
  | "audio";

interface Asset {
  id: string;
  projectId: string;
  type: AssetType;
  ownerEntityId?: string;
  label: string;
  currentVersionId?: string;
}
```

---

## 7.6 Asset Version

```ts
type AssetVersionStatus =
  | "draft"
  | "generated"
  | "candidate"
  | "qa_failed"
  | "repairing"
  | "approved"
  | "canonical"
  | "superseded";

interface AssetVersion {
  id: string;
  assetId: string;
  versionNumber: number;
  status: AssetVersionStatus;
  filePath: string;
  thumbnailPath?: string;
  parentVersionId?: string;

  generationId?: string;
  qaRunId?: string;

  createdAt: string;
}
```

Hard rule:

> Only one version of a given canonical slot may be `canonical` at a time.

Promoting a new version must supersede the old canonical version transactionally.

---

# 8. Asset State Machine

```text
DRAFT
  ↓
GENERATED
  ↓
CANDIDATE
  ├───────────────┐
  ↓               ↓
QA_FAILED       APPROVED
  ↓               ↓
REPAIRING      CANONICAL
  ↓               ↓
GENERATED      SUPERSEDED
```

Rules:
- generated output is never automatically canonical;
- QA can recommend but cannot silently approve;
- user approval is required for canon promotion in MVP;
- a repair creates a new version linked to the failed candidate;
- old canonical versions remain inspectable.

---

# 9. Provenance Model

Every generation must record:

```ts
interface Generation {
  id: string;
  projectId: string;

  workflowRunId: string;
  skillId: string;
  skillVersion: string;

  providerId: string;
  modelId: string;

  prompt: string;
  negativePrompt?: string;
  parameters: Record<string, unknown>;

  inputAssetVersionIds: string[];
  canonRevisionRefs: CanonRevisionRef[];

  outputAssetVersionIds: string[];

  startedAt: string;
  completedAt?: string;

  cost?: {
    currency: string;
    amount: number;
    credits?: number;
  };
}
```

This enables:
- reproducibility;
- compare providers;
- audit drift;
- cost analytics later;
- regenerate from exact inputs.

---

# 10. Canon Hierarchy

The application must encode these precedence rules.

## 10.1 Narrative questions

```text
Locked Story Canon
>
Draft notes
>
AI suggestions
```

## 10.2 Character appearance

```text
Latest approved canonical face asset
>
older canonical face
>
text-only visual description
```

## 10.3 Outfit appearance

```text
Canonical outfit/look asset
>
canonical face + wardrobe text
>
draft wardrobe proposal
```

## 10.4 Environment appearance

```text
Canonical world plate
>
aesthetic/location prose
```

## 10.5 Shot composition

```text
Approved shot keyframe
>
world plate
>
scene text
```

Do not silently merge conflicting authorities.

---

# 11. TBD Firewall

Canon can contain deliberate unknowns.

Example:

```ts
interface CanonTBD {
  id: string;
  topic: string;
  note?: string;
  protected: boolean;
}
```

Before generation, the production engine must run:

```text
Could this operation visually or narratively resolve a protected TBD?
```

If yes:
- block generation;
- ask user to resolve canon;
- or require explicit one-off permission that does not become canon automatically.

Example:
- room behind red door is TBD;
- user asks for “station world plate”;
- world plate must keep door opaque;
- generator must not create the room behind it.

---

# 12. Skill Runtime

## 12.1 Goal

Turn static instructions into executable production procedures.

A skill is not merely Markdown. It should have:
- identity;
- trigger;
- prerequisites;
- inputs;
- state transitions;
- prompt compiler;
- validators;
- expected outputs.

---

## 12.2 Proposed Skill Definition

```ts
interface SkillDefinition {
  id: string;
  name: string;
  version: string;
  description: string;

  operations: SkillOperation[];
}

interface SkillOperation {
  id: string;

  intentExamples: string[];

  prerequisites: Prerequisite[];

  inputSchema: JsonSchema;

  outputAssetType?: AssetType;

  workflow: WorkflowStep[];

  validators?: ValidatorDefinition[];
}
```

---

## 12.3 Example: create outfit

```text
operation:
character.create_outfit

requires:
- canonical character face exists
- wardrobe proposal is approved

inputs:
- character
- wardrobe spec
- optional garment references

workflow:
1. resolve canonical face
2. resolve relevant permanent locks
3. compile direct-on-character prompt
4. route image generator
5. save generation
6. create candidate outfit asset
7. run visual QA
8. present result
9. approve / repair / reject
```

---

# 13. Production Router

The router chooses the workflow, not the user.

Examples:

User:
> Put Mara in a raincoat.

Router:
```text
intent = wardrobe_change
character exists = true
canonical face exists = true
→ character.create_outfit
```

User:
> Put Mara inside the station at night.

Router:
```text
intent = character_scene_still
canonical character look exists = true
canonical world exists = true
→ scene.create_character_plate
```

User:
> Turn this into an 8-second video.

Router:
```text
intent = video_scene
→ cinema.compile_video
```

MVP may use an LLM classifier plus deterministic validation.

The LLM may suggest an operation, but code must validate prerequisites.

---

# 14. Provider Adapter Layer

## 14.1 Provider-neutral interface

Example:

```ts
interface ImageGenerationRequest {
  task:
    | "text_to_image"
    | "image_edit"
    | "multi_reference"
    | "character_face_lock"
    | "character_outfit"
    | "world_plate";

  prompt: string;

  references: ProviderReference[];

  requirements: {
    identityPreservation?: boolean;
    multipleReferences?: boolean;
    imageEditing?: boolean;
  };

  options?: Record<string, unknown>;
}

interface ImageProviderAdapter {
  capabilities(): ProviderCapabilities;
  generate(req: ImageGenerationRequest): Promise<ProviderResult>;
}
```

---

## 14.2 Initial adapters

MVP should implement only:

1. `MockImageProvider`
2. `OneCloudImageProvider`
3. `ComfyUIProvider` or local stub if feasible

Do not integrate ten providers before the workflow works.

Video provider integration comes later.

---

# 15. Provider Router

Routing inputs:
- capability;
- user preference;
- privacy;
- local/cloud policy;
- cost;
- model availability.

Future policy:

```text
Local preferred
→ if capability unavailable
→ cloud fallback
```

MVP:
- user picks default provider;
- router checks capability;
- error clearly if unsupported.

---

# 16. Local AI Responsibilities

Local AI should initially handle:
- canon summarization/retrieval;
- intent routing;
- prompt compilation;
- visual QA;
- embeddings;
- asset tagging;
- metadata extraction;
- repair instruction generation.

Do not require local frontier-quality image/video generation for MVP.

That keeps the app useful on moderate hardware.

---

# 17. Visual QA Engine

This is a major differentiator.

## 17.1 Inputs

```text
candidate asset
+
canonical reference assets
+
visual locks
+
operation-specific expectations
```

---

## 17.2 Example QA output

```json
{
  "overall": "fail",
  "checks": [
    {
      "id": "identity_similarity",
      "status": "pass",
      "confidence": 0.95
    },
    {
      "id": "right_eyebrow_scar",
      "status": "fail",
      "reason": "Scar appears on character-left eyebrow."
    },
    {
      "id": "watch_left_wrist",
      "status": "pass"
    },
    {
      "id": "unexpected_artifact",
      "status": "fail",
      "reason": "Sparkle-shaped mark detected in lower-right."
    }
  ]
}
```

---

## 17.3 QA categories

Initial checks:
- identity similarity;
- permanent visual locks;
- hair;
- skin register;
- outfit pieces;
- accessory side/placement;
- required props;
- forbidden elements;
- flat reference background;
- watermark/artifact detection;
- composition requirements.

Do not promise pixel-perfect computer vision. QA results need confidence and human confirmation.

---

# 18. Repair Workflow

When QA fails:

```text
candidate
   ↓
failed checks
   ↓
repair compiler
   ↓
minimal edit prompt
   ↓
image-edit provider
   ↓
new candidate version
   ↓
QA again
```

Core repair principle:

> Preserve everything already correct. Change only failed conditions.

Example:

```text
Preserve identity, wardrobe, pose, framing, and background.
Make only these corrections:
1. Move scar to character-right eyebrow.
2. Remove lower-right artifact.
```

This should be generated from structured failed checks, not improvised from scratch.

---

# 19. Canon Engine

The Canon Engine should support:

- story canon;
- characters;
- locations;
- aesthetic;
- world rules;
- production rules;
- protected TBDs;
- narrative revisions.

MVP does not need an elaborate knowledge graph.

Use typed JSON structures with relationships by IDs.

---

# 20. Story Bible Workflow

Initial implementation can be a guided editor rather than a fully autonomous interview.

Sections:
1. premise
2. thesis
3. timeline
4. aesthetic
5. factions optional
6. locations
7. world rules
8. characters
9. relationships optional
10. structural engines optional
11. production rules
12. active-skill/runtime rules

For MVP, support:
- editable fields;
- lock/unlock;
- revision history;
- AI-assisted suggestions;
- export Markdown.

---

# 21. Character Workflow

Canonical MVP workflow:

```text
Character Draft
   ↓
Text Visual Spec
   ↓
FACE LOCK
   ↓
QA
   ↓
CANONICAL FACE
   ↓
Wardrobe Proposal
   ↓
Approval
   ↓
OUTFIT
   ↓
QA
   ↓
CANONICAL LOOK
   ↓
CHARACTER SHEET
```

Hard prerequisites:
- outfit cannot start without canonical face;
- character sheet cannot start without approved/canonical outfit;
- permanent face change creates a new face version.

---

# 22. Face Lock Rules

The reference plate should have:
- flat neutral gray field;
- flat shadowless light;
- zero cast/contact shadow;
- no scene lighting;
- no atmospheric haze;
- no cinematic depth of field;
- biological realism preserved.

Why:
reference assets should carry identity, not accidental scene-light conditioning.

---

# 23. Outfit Rules

Default:
**direct on canonical character**.

Do not require a stand-in model first.

Fallback:
- if one garment fails → garment repair workflow;
- if entire aesthetic concept is unresolved → optional concept-generation workflow later.

---

# 24. Character Sheet

Default 3-panel:
1. full-body front, headless;
2. full-body rear;
3. tight chest-up face.

Why:
one image has finite pixel budget; three panels preserve more facial resolution than six.

Six-panel can be added post-MVP.

---

# 25. World Plate vs Shot Keyframe

These are separate asset types.

## World Plate

Purpose:
persistent environment truth.

Contains:
- architecture;
- geography;
- materials;
- set dressing;
- motivated practical lighting;
- baseline atmosphere.

Should not over-lock:
- one specific lens;
- exact character placement;
- a single shot composition.

## Shot Keyframe

Purpose:
specific frame truth.

Contains:
- exact subject position;
- pose;
- camera angle;
- framing;
- shot-specific light;
- spatial blocking.

This distinction prevents a world reference from accidentally forcing every video shot into one camera setup.

---

# 26. Scene Model

```ts
interface Scene {
  id: string;
  projectId: string;
  title: string;

  characterAssignments: SceneCharacterAssignment[];
  worldAssetVersionId?: string;
  propAssetVersionIds: string[];

  canonNotes?: string;
  tbdRefs: string[];
}
```

Character assignment should reference:
- character ID;
- look asset version;
- optional sheet asset version.

---

# 27. Shot Model

```ts
interface Shot {
  id: string;
  sceneId: string;
  order: number;

  durationSeconds?: number;

  keyframeAssetVersionId?: string;

  intent: string;
  action?: string;
  camera?: string;

  generatedVideoAssetVersionId?: string;
}
```

---

# 28. Cinema Compiler

MVP output is a **provider-neutral video production prompt**, not necessarily direct video generation.

Inputs:
- Story Canon;
- Character Speech/Movement/Stillness;
- current character sheet/look;
- world plate;
- props;
- requested duration/action.

Output:
- shot count;
- durations;
- subject locks;
- camera;
- performance;
- continuity;
- audio instructions;
- last frame;
- provider prompt.

Later, provider-specific compiler adapters can transform this into model-specific syntax.

---

# 29. UI Information Architecture

## 29.1 Main shell

```text
┌─────────────────────────────────────────────────────┐
│ PROJECT NAME                                        │
├──────────────┬──────────────────────┬───────────────┤
│ NAVIGATION   │ WORKSPACE            │ INSPECTOR     │
│              │                      │               │
│ Story        │ asset/grid/editor    │ status        │
│ Characters   │                      │ version       │
│ Worlds       │                      │ QA            │
│ Scenes       │                      │ provenance    │
│ Assets       │                      │ references    │
├──────────────┴──────────────────────┴───────────────┤
│ AI DIRECTOR / COMMAND BAR                           │
└─────────────────────────────────────────────────────┘
```

---

## 29.2 Character page

```text
MARA KEENE
THE VERIFIER

[portrait]

Identity
● FACE-V01      CANONICAL

Looks
● SHIFT-V01     CANONICAL
○ RAIN-V01      DRAFT

Sheets
● SHIFT-SHEET-V01

Narrative
- Psychology
- Speech
- Movement
- Stillness

Permanent Locks
✓ eyebrow scar
✓ hair
✓ skin register
```

---

## 29.3 Asset inspector

Show:
- preview;
- asset ID;
- version;
- status;
- owner;
- creation model/provider;
- prompt;
- input references;
- QA;
- parent version;
- children;
- promote canonical;
- repair;
- supersede;
- open file.

---

# 30. MVP Scope

MVP is complete when user can:

1. Create a project.
2. Define story canon.
3. Create one character.
4. Store permanent visual locks.
5. Create/import a face-lock candidate.
6. Run QA.
7. Repair a failed candidate.
8. Promote a face as canonical.
9. Define an outfit.
10. Generate/import outfit candidate.
11. QA and canonicalize outfit.
12. Generate/import character sheet.
13. Create/import world plate.
14. Create a scene linking character look + world.
15. Compile a cinema/video prompt.
16. Inspect full provenance for every generated asset.

---

# 31. Explicit Non-Goals for MVP

Do not implement:
- cloud collaboration;
- login/auth;
- subscription billing;
- public marketplace;
- multi-user projects;
- built-in video timeline editing;
- video compositing;
- full audio editor;
- model training;
- LoRA training;
- hosted GPU infrastructure;
- mobile;
- social sharing;
- automatic cloud backup;
- dozens of providers;
- fully autonomous filmmaking agent.

---

# 32. Implementation Strategy

Use incremental sub-projects.

Do not attempt the entire product in one coding pass.

Recommended order:

```text
P0 Project Kernel
P1 Asset Manifest + Versioning
P2 Canon Engine
P3 Skill/Workflow Runtime
P4 Provider Adapter
P5 Character Pipeline
P6 Visual QA + Repair
P7 World/Scene Pipeline
P8 Cinema Compiler
P9 Integration/Polish
```

Each phase must be usable and testable before the next begins.

---

# 33. P0 — Project Kernel

## Goal

User can create/open a local cinematic project.

### Deliverables

- Tauri shell
- create project
- open project
- recent projects
- project path validation
- database initialization
- migration runner
- project metadata
- basic navigation

### Acceptance

Given a new empty directory:
- app creates a valid project;
- closes;
- reopens;
- project metadata persists;
- invalid/corrupt project reports useful error.

---

# 34. P1 — Asset Manifest + Versioning

## Goal

The application can manage media assets independently of generation.

### Deliverables

- asset import
- thumbnails
- asset type
- asset owner
- version creation
- asset status
- canonical promotion
- superseding
- provenance placeholder
- asset inspector

### Acceptance

User can:
- import image;
- classify as Face Lock;
- create V02;
- promote V01 canonical;
- promote V02 canonical;
- see V01 become superseded;
- never lose old file/history.

---

# 35. P2 — Canon Engine

## Goal

Structured story/character/world state.

### Deliverables

- Story Canon editor
- Character Canon editor
- visual locks
- TBD entries
- lock/unlock sections
- revision history
- Markdown export

### Acceptance

- locked section cannot be changed without explicit unlock;
- character visual locks are queryable by QA;
- TBDs are queryable by workflow validation;
- Markdown export contains coherent bible.

---

# 36. P3 — Workflow / Skill Runtime

## Goal

Run at least one executable skill operation.

Start with:

`character.create_face_lock`

### Deliverables

- SkillDefinition schema
- registry
- prerequisite validation
- workflow run persistence
- prompt compiler
- workflow UI state

### Acceptance

If character lacks visual spec:
- workflow blocks and explains prerequisite.

If valid:
- compile prompt;
- record skill version;
- produce provider request.

---

# 37. P4 — Provider Adapter

## Goal

One real generation path works end-to-end.

### Deliverables

- provider interface
- mock provider
- one real image provider
- API key settings
- capability declaration
- request/result normalization
- generation record

### Acceptance

User runs face-lock workflow:
- provider receives compiled prompt;
- output downloaded to project;
- asset version created;
- provenance stored.

If API fails:
- no phantom asset created;
- error is recoverable;
- workflow can retry.

---

# 38. P5 — Character Pipeline

## Goal

Face → Outfit → Sheet.

### Deliverables

- face workflow
- canonical approval
- wardrobe proposal UI
- outfit generation
- character sheet generation
- prerequisite enforcement
- look versioning

### Acceptance

Cannot:
- create outfit before face is canonical;
- create sheet before outfit is approved.

Can:
- create multiple looks;
- maintain separate canonical versions.

---

# 39. P6 — Visual QA + Repair

## Goal

Detect common drift and compile repairs.

### Deliverables

- QA schema
- local/cloud VLM adapter
- visual-lock checks
- artifact check
- structured QA result
- repair prompt compiler
- repair workflow

### Acceptance Scenario

Character requires:
- right eyebrow scar;
- watch on left wrist.

Generated candidate:
- scar mirrored;
- extra mark in corner.

QA returns both failures.

Repair:
- instructs only those corrections;
- preserves successful traits;
- creates child asset version;
- reruns QA.

---

# 40. P7 — World + Scene Pipeline

## Goal

Create canonical environment and assemble scene references.

### Deliverables

- world/location entity
- world plate asset
- world plate workflow
- Scene model/UI
- character assignment
- prop assignment
- TBD firewall check
- shot-keyframe asset

### Acceptance

Scene references:
- exact look version;
- exact world version;
- props.

Changing world canonical version must not silently rewrite old scenes. Existing scenes keep their explicit version reference until user upgrades them.

---

# 41. P8 — Cinema Compiler

## Goal

Compile a structured video prompt.

### Deliverables

- shot model
- runtime validation
- character behavior retrieval
- world retrieval
- cross-frame continuity compiler
- provider-neutral cinema prompt
- prompt export

### Acceptance

Given:
- one character sheet;
- one world plate;
- 8-second scene request;

compiler produces:
- coherent time budget;
- character behavioral locks;
- shot instructions;
- world continuity;
- no unapproved TBD resolution.

---

# 42. Testing Strategy

Follow TDD where domain logic is deterministic.

## 42.1 Unit tests

Mandatory for:
- state transitions;
- canon promotion;
- superseding;
- prerequisites;
- hierarchy resolution;
- TBD firewall;
- provider capability selection;
- prompt-input assembly;
- provenance records.

## 42.2 Integration tests

Test:
- create/open project;
- DB migrations;
- import asset;
- generation result → asset;
- QA → repair;
- canonical promotion;
- scene reference resolution.

## 42.3 Provider contract tests

Use recorded fixtures or mock adapters.

Do not make paid provider calls in normal automated tests.

## 42.4 UI tests

Critical flows:
- project creation;
- asset import;
- canonical promotion;
- workflow launch;
- QA result;
- repair;
- scene assembly.

---

# 43. Error Handling Rules

Never silently:
- drop references;
- replace canonical assets;
- resolve TBDs;
- switch provider;
- retry paid generation;
- overwrite files.

All generation jobs need explicit status:

```text
queued
running
succeeded
failed
cancelled
```

On crash/restart:
- recover job state;
- preserve outputs already written;
- avoid duplicate charges where possible.

---

# 44. Security / Privacy

MVP:
- credentials stored in OS secure credential storage;
- never in project files;
- redact API keys in logs;
- local project paths remain local;
- provider adapter clearly marks which media leaves device.

UI should show:

```text
LOCAL
or
CLOUD: Provider Name
```

before generation.

Later add privacy routing policies.

---

# 45. Performance

Do not prematurely optimize.

Required:
- lazy thumbnail loading;
- media files stay outside DB;
- background generation jobs;
- no blocking UI during AI calls;
- image hashing to detect duplicate imports;
- async thumbnail generation.

---

# 46. Observability

Local logs should include:
- workflow run;
- provider request ID;
- asset IDs;
- generation duration;
- QA duration;
- failure stage.

Do not log secrets.

Optional debug export:
`project-diagnostics.zip`.

---

# 47. Naming Convention

Human-readable asset aliases:

```text
MARA-FACE-V01
MARA-SHIFT-LOOK-V01
MARA-SHIFT-SHEET-V01
STATION-WORLD-V01
SCENE-001-SHOT-001-KEYFRAME-V01
```

Internal IDs must be immutable UUID/ULID.

Never use display names as foreign keys.

---

# 48. Example End-to-End Project

This sample should be included as fixture/demo data.

## Story

A lone coastal-radio operator receives transmissions in her own voice from minutes in the future. Each verified prediction makes the signal more credible. The voice repeatedly warns her not to open a red maintenance door.

## Character

**Mara Keene — The Verifier**

Narrative:
- disciplined verification;
- fear creates greater precision;
- competence becomes the trap.

Permanent visual locks:
- near-black shoulder-length hair;
- warm light-medium skin;
- small scar through character-right eyebrow;
- no bangs;
- restrained neutral expression.

Face asset:
`MARA-FACE-V01`

Look:
`MARA-SHIFT-LOOK-V01`

Wardrobe:
- charcoal long-sleeve top;
- dark utility trousers;
- black boots;
- black watch on left wrist.

## World

`STATION-WORLD-V01`

Geography:

```text
Main Operations Room
        ↓
Equipment Corridor
        ↓
Red Door
        ↓
[TBD / unseen]
```

Protected TBD:
- what is behind the red door.

This demo is useful for development because it exercises:
- side-specific identity locks;
- outfit locks;
- TBD firewall;
- scene geography;
- prompt compilation.

---

# 49. Initial Database Tables

Suggested starting schema:

```text
projects
canon_entities
canon_revisions
canon_tbds
visual_locks

assets
asset_versions
asset_relationships

workflow_runs
workflow_steps

generations
generation_inputs
generation_outputs

provider_configs
provider_runs

qa_runs
qa_checks

scenes
scene_characters
scene_props
shots
```

Normalize where useful, but avoid building an abstract graph database prematurely.

SQLite is sufficient.

---

# 50. Asset Relationships

Use explicit relationship types.

Examples:

```text
DERIVED_FROM
USES_IDENTITY
USES_LOOK
USES_WORLD
USES_PROP
REPAIRS
SUPERSEDES
SHEET_FOR_LOOK
KEYFRAME_FOR_SHOT
```

This enables provenance queries without requiring a graph database.

---

# 51. Workflow Run Model

A workflow execution must be resumable.

```ts
interface WorkflowRun {
  id: string;
  skillId: string;
  operationId: string;
  state: string;

  input: Record<string, unknown>;
  contextSnapshot: WorkflowContextSnapshot;

  createdAt: string;
  updatedAt: string;
}
```

`contextSnapshot` stores exact canon/asset refs at launch so later canon changes do not mutate historical runs.

---

# 52. Approval Semantics

MVP approval is human-driven.

Buttons:
- Reject
- Repair
- Approve
- Promote to Canon

`Approve` means output is good.
`Promote to Canon` means it becomes source of truth.

For simple workflows these can be combined later, but keep semantic distinction in domain model.

---

# 53. LLM Role

The LLM is allowed to:
- propose;
- summarize;
- classify intent;
- compile prompts;
- explain QA;
- suggest repairs.

The LLM must not:
- directly mutate locked canon without explicit mutation command;
- promote assets;
- overwrite versions;
- resolve protected TBDs;
- choose paid retries silently.

Business rules remain deterministic code.

---

# 54. VLM Role

The VLM acts as an evaluator, not ultimate authority.

It produces:
- observed traits;
- comparison;
- confidence;
- failures;
- evidence description.

Human can override.

Store both:
- VLM result;
- user decision.

This allows future QA-model improvement.

---

# 55. Prompt Compiler

Prompts should be generated from:
1. operation;
2. relevant canon only;
3. relevant references only;
4. current requested delta;
5. provider capability.

Do not concatenate all project context.

Prompt economy is a product feature.

Compiler pipeline:

```text
resolve operation
→ resolve authority
→ collect minimal references
→ collect immutable locks
→ collect requested change
→ protect TBDs
→ compile provider-neutral prompt
→ adapt provider syntax
```

---

# 56. Reference Economy

When multiple references exist, select the fewest coherent references.

Example character sheet:
- default: approved outfit render only;
- add face lock only if identity drift exists.

Video:
- character sheet;
- world plate;
- essential prop;
- audio if needed.

Do not attach every asset “for safety”.

---

# 57. Development Rules for AI Coding Agents

Any AI implementing this plan must follow these rules:

1. Do not invent product requirements not in this plan.
2. If a detail is ambiguous, prefer the simplest implementation satisfying current acceptance criteria.
3. Use explicit types rather than generic JSON where domain type is known.
4. Write migrations, not ad hoc DB mutations.
5. Keep provider-specific code out of domain packages.
6. Keep React components free of core state-machine logic.
7. Add tests for every important domain transition.
8. Never silently overwrite user media.
9. Every implementation phase must leave the app runnable.
10. Do not implement future phases early unless required by an interface boundary.
11. No marketplace/cloud-auth/billing work in MVP.
12. Prefer boring technology over speculative infrastructure.

---

# 58. Recommended First Implementation Sprint

Do **not** begin with AI generation.

Start with the substrate.

## Sprint 1 tasks

### Task 1 — Bootstrap monorepo
Create:
- Tauri desktop app;
- React TypeScript UI;
- shared package setup;
- Python worker placeholder.

### Task 2 — Project domain
Implement:
- `Project`;
- create/open;
- project path;
- project metadata.

### Task 3 — SQLite
Implement migrations and repositories.

### Task 4 — Project filesystem
Create deterministic directory structure.

### Task 5 — Asset domain
Implement:
- Asset;
- AssetVersion;
- status enum;
- relationship enum.

### Task 6 — Asset import
Import image into project-managed location, hash it, create thumbnail.

### Task 7 — Asset Inspector
Display metadata/version/file.

### Task 8 — Canonical promotion
Implement transactional promotion + superseding.

### Task 9 — Tests
Unit/integration tests for all state transitions.

### Sprint 1 Done Condition

A user can:
- create project;
- import two images as versions of a Face asset;
- promote V01;
- promote V02;
- inspect that V01 is superseded and V02 canonical;
- close/reopen project with all state intact.

Only after this works should implementation move to Canon Engine.

---

# 59. Definition of MVP Done

MVP is done when a fresh user can complete the entire canonical workflow without manually tracking state outside the app:

```text
Story Canon
→ Character Canon
→ Face Lock
→ QA
→ Repair
→ Canonical Face
→ Outfit
→ QA
→ Canonical Outfit
→ Character Sheet
→ World Plate
→ Scene
→ Cinema Prompt
```

Every asset:
- has a version;
- has provenance;
- has explicit status;
- can be inspected;
- can be superseded;
- cannot silently replace canon.

Every generation:
- records provider/model/prompt/input refs;
- can be traced to canon;
- respects TBD firewall.

This is the MVP quality bar.

---

# 60. Post-MVP Roadmap

Only after MVP validation:

## V1.1
- multiple providers;
- provider comparison;
- cost tracking.

## V1.2
- local ComfyUI workflows;
- provider auto-routing.

## V1.3
- direct video generation;
- video QA.

## V1.4
- storyboard/shot board;
- batch generation.

## V1.5
- cloud sync;
- collaboration.

## V1.6
- skill/plugin ecosystem.

## V2
Potential marketplace/runtime platform for third-party production workflows.

Do not commit to these until the core workflow demonstrates real user value.

---

# 61. Core Product Moat

The moat should be understood as:

> **Accumulated production knowledge encoded into stateful workflows, canonical assets, validation rules, repair logic, and provider-independent execution.**

Not:
- prompt templates;
- one image model;
- one video model;
- chat UI.

A model can be replaced.
A provider can disappear.
The canonical production graph remains.

---

# 62. Final Engineering Thesis

Build the product so that the following statement remains true:

> A user can switch the underlying image or video model tomorrow without rebuilding their story world, character canon, asset history, workflow state, or production logic.

If architecture prevents that, provider concerns have leaked too far into the core and should be refactored.

---

# 63. Immediate Next Action for Implementer

Start **P0 / Sprint 1 only**.

Do not implement Canon AI, Skill Runtime, providers, or QA yet.

First establish:
- desktop shell;
- project storage;
- SQLite;
- asset versioning;
- canonical promotion;
- persistence;
- tests.

Once Sprint 1 passes its Done Condition, create the implementation plan for **P2 Canon Engine** using the interfaces established by P0/P1 rather than guessing them in advance.

