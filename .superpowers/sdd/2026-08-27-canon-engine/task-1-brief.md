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

