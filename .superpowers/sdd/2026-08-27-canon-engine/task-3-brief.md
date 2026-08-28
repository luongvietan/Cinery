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

