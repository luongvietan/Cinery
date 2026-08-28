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

