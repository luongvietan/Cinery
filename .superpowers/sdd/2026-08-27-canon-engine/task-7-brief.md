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

