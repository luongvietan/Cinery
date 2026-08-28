# Character Workflow Results and Promotion Design

## Purpose

Provide one complete, user-operable Face Lock to Outfit to Character Sheet production flow. Every operation must expose provider selection, generated candidates, explicit version creation, and optional canonical promotion without requiring manual database work or an unrelated screen.

## Current State

- Face Lock, Outfit, and Character Sheet workflow definitions and backend prerequisites exist.
- Production only launches Face Lock, hard-codes mock, and requires an existing canonical Face as its target.
- Workflow forms can launch Outfit and Character Sheet, but completed runs do not expose their generated result sets or promotion controls.
- `GenerationResults` and `PromoteArtifactDialog` assume a Face Lock target asset.

## Product Flow

Production is the guided creation surface. Workflows remains the operational history and debugging surface. Both can open a completed run and show the same reusable result panel.

Each character operation follows this sequence:

```text
Choose operation and character
→ provide operation input
→ choose provider and model
→ review immutable context and compiled request
→ explicitly approve execution
→ inspect generated candidates
→ choose or create a compatible target Asset
→ save as a new candidate AssetVersion
→ optionally promote that exact version to Canon
```

No step automatically promotes Canon.

## Operation Rules

### Face Lock

- Requires a Character Canon entity and complete visual specification.
- An existing canonical Face reference is optional. It is offered for identity-preserving regeneration, not required for the first Face.
- Output type is `face_lock`, owned by the selected character.

### Outfit

- Requires the selected character's current canonical Face.
- Captures the exact Face AssetVersion in the workflow snapshot.
- Output type is `outfit`, owned by the selected character.

### Character Sheet

- Requires the selected character's current canonical Outfit.
- Captures the exact Outfit and associated Face references in the workflow snapshot.
- Output type is `character_sheet`, owned by the selected character.

## Shared Result Model

Generalize the result UI around workflow metadata rather than labels embedded in components.

The panel consumes:

```ts
interface GenerationResultContext {
  workflowRunId: string;
  operationId: string;
  expectedAssetType: AssetType;
  ownerEntityId: string | null;
  resultSets: GenerationResultSetDetail[];
}
```

Eligible target assets must belong to the open project, match `expectedAssetType`, and match `ownerEntityId`. Backend promotion repeats these checks; the UI filter is convenience, not authority.

If no eligible target exists, the result panel offers an inline create-asset form with a meaningful default label. After creation, the new asset becomes the selected promotion target. The user then chooses whether the saved version remains candidate or becomes canonical.

Already promoted artifacts remain visible but cannot be promoted twice. Failed or unavailable artifacts show their capture status and do not expose the save action.

## UI Structure

- Production lists all three Character Builder operations with prerequisite state and blocker text.
- A single launch shell renders the operation-specific fields plus the shared provider/model control.
- Workflow run review continues to show context, prompt, approval, execution, retry, cancel, and event history.
- A completed generation run renders the shared result panel below its run history in both Production and Workflows.
- Result cards show image preview, ordinal, provider/model, exact source references, dimensions, capture status, and promotion status.
- Promotion remains an explicit inline confirmation or focused dialog with keyboard focus management.

The UI follows the existing restrained visual system. It uses production-oriented lists, preview grids, and inspectors rather than nested generic cards.

## State and Recovery

- Reloading or reopening the project reconstructs result galleries from persisted result sets.
- A successful generation with no promotion remains a valid completed run.
- Promotion retries are idempotent.
- Provider retry creates a new attempt and result set; older candidate sets remain inspectable.
- Switching character or operation clears stale target selections and pending form input.
- A canonical version changed after workflow launch does not rewrite the snapshot or generated artifact lineage.

## Test Strategy

- Backend tests cover optional Face source, required Outfit/Sheet prerequisites, target asset type/owner validation, and idempotent promotion.
- Component tests cover provider selection, each operation form, result rendering, inline target creation, candidate save, canonical promotion, and restart reload.
- A higher-level UI test exercises the three operations with a stateful fake command layer rather than independent static API mocks.
- The full command-boundary acceptance chain is owned by the release validation spec.

## Acceptance Criteria

1. A new character can create its first Face Lock without already owning a canonical Face.
2. A user can generate, inspect, save, and promote Face, Outfit, and Character Sheet results from the desktop UI.
3. Outfit cannot launch without a canonical Face; Sheet cannot launch without a canonical Outfit.
4. Every promoted output retains provider, model, prompt, workflow, Canon snapshot, and exact input references.
5. Completed result galleries survive application restart.

## Non-Goals

- Batch generation across multiple characters.
- Automatic candidate ranking or automatic promotion.
- Multiple simultaneous canonical Outfit versions for one Asset.
- Redesigning Canon editors.
