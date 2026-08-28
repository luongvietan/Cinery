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

