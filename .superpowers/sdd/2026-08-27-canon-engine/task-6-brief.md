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

