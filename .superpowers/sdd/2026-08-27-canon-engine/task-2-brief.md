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

