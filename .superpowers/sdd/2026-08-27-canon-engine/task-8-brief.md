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

