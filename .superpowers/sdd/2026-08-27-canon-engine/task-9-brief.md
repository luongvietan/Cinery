## Task 9 — End-to-end Canon Engine acceptance and restart verification

**Files:**
- Create: `apps/desktop/src-tauri/tests/canon_engine_acceptance.rs`
- Create: `docs/superpowers/plans/canon-engine-verification.md`
- Modify: `README.md`

**Produces:** proof that P2 works independently and is safe for Skill Runtime planning.

- [ ] **Step 1: Write the full acceptance test**

Scenario:

1. Create project `Red Door`.
2. Ensure Story + Production Rules singletons.
3. Set Premise.
4. Lock Premise.
5. Set Thesis but leave Draft.
6. Create Character `Mara Keene`.
7. Set Role Tag `The Verifier`; lock.
8. Set Function; lock.
9. Add Visual Locks:
   - `right_eyebrow_scar`
   - `no_bangs`
10. Lock Visual Locks.
11. Create Location `The Station`.
12. Set Geography; lock.
13. Create World Rule:
    - anomaly uses radio infrastructure;
    - lock Rule.
14. Add Production Rule:
    - unknown canon stays unknown;
    - lock.
15. Create protected TBD:
    - what is behind the red door?
16. Export Story Bible.
17. Drop all service/DB scopes.
18. Reopen project.
19. Verify all IDs/revisions/statuses.
20. Export again.
21. Verify export bytes equal.

- [ ] **Step 2: Assert locked edit protection after reopen**

After reopen:

Attempt direct service edit of locked Premise.

Expected:

```text
CanonSectionLocked
```

Then:
- unlock;
- edit;
- revision increases.

- [ ] **Step 3: Assert revision history survives reopen**

Premise history must show:

```text
create
lock
unlock
edit
```

with contiguous revisions.

- [ ] **Step 4: Assert future query boundaries**

Call:

```rust
get_locked_character_visual_locks(...)
list_locked_world_rules(...)
get_locked_production_rules(...)
list_open_protected_tbds(...)
```

Expected:
- only locked canon returned;
- draft thesis excluded;
- protected TBD returned.

This proves P2 exposes the correct future Skill Runtime inputs without implementing Skill Runtime itself.

- [ ] **Step 5: Create manual desktop verification guide**

Create:

```text
docs/superpowers/plans/canon-engine-verification.md
```

Checklist:

```markdown
# Canon Engine Desktop Verification

1. Open Sprint 1 project or create `Red Door`.
2. Open Canon → Story.
3. Enter Premise.
4. Save.
5. Lock Premise.
6. Confirm editor becomes read-only.
7. Open History and confirm Create + Lock revisions.
8. Unlock Premise.
9. Edit one sentence.
10. Save and confirm revision increments.
11. Create Character `Mara Keene`.
12. Set Role Tag `The Verifier`.
13. Add a required visual lock `right_eyebrow_scar`.
14. Lock Visual Locks.
15. Create Location `The Station`.
16. Enter and lock Geography.
17. Create a World Rule and lock it.
18. Add one Production Rule and lock it.
19. Add protected TBD `What is behind the red door?`.
20. Export Story Bible.
21. Open exported Markdown and verify all content.
22. Fully close app.
23. Reopen project.
24. Verify every canon section status and revision persists.
25. Verify the protected TBD remains open.
26. Export again.
27. Confirm exported Story Bible content is unchanged.
```

- [ ] **Step 6: Update README**

Add Canon Engine architecture notes:

```text
Locked sections = canonical.
Draft sections = working state.
Markdown = export, not source of truth.
Protected TBDs = future workflow firewall.
```

Document tests.

- [ ] **Step 7: Run full verification**

```bash
pnpm install
pnpm test
pnpm test:rust
pnpm --filter @cinematic/desktop tauri build --debug
```

Then:

```bash
pnpm dev
```

Complete manual checklist.

- [ ] **Step 8: Commit P2 acceptance**

```bash
git add README.md apps/desktop/src-tauri/tests docs/superpowers/plans
git commit -m "test: verify canon engine persistence"
```

**Task 9 acceptance:** structured Canon survives restart, locked state is enforced server-side, revisions are append-only, protected TBDs are queryable, and Markdown export is deterministic.

---

# 17. Canon Engine Acceptance Matrix

| Requirement | Test |
|---|---|
| Story Canon structured | Story editor + acceptance |
| Character Canon structured | Character editor + acceptance |
| Locations | Task 6 |
| Factions | Task 6 |
| World Rules | Task 6 |
| Production Rules | Task 6 |
| Section lock/unlock | Task 3 |
| Locked section cannot mutate | Task 3 + Task 9 |
| Revision history | Task 3 + Task 9 |
| Permanent visual locks | Task 5 |
| QA-queryable visual locks | Task 5 + Task 9 |
| Protected TBD | Task 7 |
| Open protected query | Task 7 + Task 9 |
| Restart persistence | Task 9 |
| Markdown export | Task 8 |
| Deterministic export | Task 8 + Task 9 |
| No AI/provider dependency | architecture inspection |

---

# 18. Cross-Task Invariants

Every task must preserve:

- [ ] SQLite remains the machine source of truth.
- [ ] Markdown never becomes mutable canonical storage.
- [ ] Canon entity IDs are immutable ULIDs.
- [ ] Slugs are display/navigation metadata, never foreign keys.
- [ ] Section status is only `draft` or `locked`.
- [ ] Locked section cannot be edited.
- [ ] Unlock is explicit.
- [ ] Every successful create/edit/lock/unlock creates exactly one revision.
- [ ] Revision history is append-only.
- [ ] Section value must validate against entity-type + section-key schema.
- [ ] Unknown section keys are rejected.
- [ ] Visual locks have unique keys inside a Character visual-lock section.
- [ ] Only locked visual locks are exposed to future QA query.
- [ ] Protected open TBDs are queryable project-wide.
- [ ] Resolving a TBD never silently edits canon.
- [ ] P2 does not call any AI model.
- [ ] P2 does not know provider/model concepts.
- [ ] Asset canonical state from Sprint 1 remains untouched.
- [ ] Existing project files and media are never overwritten by Canon operations.
- [ ] Canon export uses atomic temp-file rename.

---

# 19. P2 Definition of Done

P2 is complete only when:

1. A project automatically has stable Story and Production Rules canon containers.
2. A user can create Character, Location, Faction, and World Rule entities.
3. Story sections are independently editable.
4. Story sections are independently lockable.
5. Locked sections reject backend mutations.
6. Unlock is explicit and revisioned.
7. Every create/edit/lock/unlock has append-only history.
8. Character Speech/Movement/Stillness are stored as prompt-ready structured fields.
9. Permanent Character visual locks are structured, not only prose.
10. Locked visual locks are queryable for future QA.
11. Location geography can be locked independently.
12. World Rules can be authored and locked individually.
13. Production Rules can be authored and locked.
14. Protected TBDs can be created.
15. Protected TBDs can be scoped to project/entity/section.
16. TBDs can be resolved without silently mutating canon.
17. TBDs can be reopened.
18. Open protected TBDs are queryable.
19. Story Bible Markdown export is deterministic.
20. Export represents missing canon as `[TBD]` rather than inventing data.
21. Canon state survives complete application restart.
22. Section history survives restart.
23. Existing Sprint 1 asset state remains unaffected.
24. TypeScript tests pass.
25. Rust tests pass.
26. Canon acceptance test passes.
27. Tauri debug build succeeds.
28. Manual desktop verification passes.

---

# 20. What Must Not Leak Into P2

Reject implementation attempts that introduce any of these:

```text
OpenAI client
Gemini client
Ollama client
ComfyUI client
prompt generation
embeddings
vector DB
SkillDefinition
workflow execution
provider capability router
asset QA
face comparison
image generation
video generation
scene generation
Cinema Director
automatic Story Bible interview
```

If an implementation task appears to require one of these, stop and reassess. Canon Engine must remain deterministic and model-agnostic.

---

# 21. Recommended Commit Sequence

Expected commits:

```text
feat: add canon engine schemas
feat: add canon entity management
feat: add canon section locking and revisions
feat: add story canon editor
feat: add character canon and visual locks
feat: add world and production canon editors
feat: add protected canon TBDs
feat: export structured canon as story bible
test: verify canon engine persistence
```

Keep commits narrow enough to review independently.

---

# 22. Self-Review

## 22.1 Spec coverage

Master P2 requirements mapped:

- structured Story Canon → Tasks 1–4.
- Character Canon → Task 5.
- visual locks → Task 5.
- Locations → Task 6.
- Factions → Task 6.
- World Rules → Task 6.
- Production Rules → Task 6.
- lock/unlock → Task 3.
- revision history → Task 3.
- TBD entries → Task 7.
- protected TBD firewall query → Task 7.
- Markdown export → Task 8.
- restart persistence → Task 9.
- no AI dependency → global constraints + Task 9 inspection.

No P2 requirement is deferred.

## 22.2 Placeholder scan

Implementation steps contain:
- exact files;
- exact schemas;
- exact transitions;
- explicit error cases;
- test commands;
- acceptance conditions.

No vague “add validation”, “write tests”, or “similar to previous task” instructions remain.

## 22.3 Type/signature consistency

Checked:
- `CanonEntity` identifiers and types are consistent.
- Singleton entity types are consistently `story` and `production_rules`.
- Section state is consistently `draft | locked`.
- Revision change kinds are consistently `create | edit | lock | unlock`.
- Visual locks are stored in Character `visual_locks`.
- TBD status is consistently `open | resolved`.
- Story Bible export path is consistently `canon/story-bible.md`.
- Frontend command wrapper names correspond to explicit Tauri command names.
- Later query helpers return locked canon only.

No signature conflict remains.

---

# 23. Execution Handoff

When implementing this plan:

1. Work in an isolated git worktree.
2. Use `superpowers:subagent-driven-development` if available.
3. Give one task to one fresh implementation subagent.
4. Run the task's specified tests before review.
5. Review code against both:
   - task acceptance;
   - Cross-Task Invariants.
6. Commit before moving to next task.
7. Never begin the next architectural phase while P2 acceptance is failing.

After P2 passes completely:

> **Stop.**

The next plan should be **P3 — Skill / Workflow Runtime**.

Do not implement Skill Runtime opportunistically inside Canon Engine.

The P3 planning session must consume the *actual implemented* Canon Engine interfaces, especially:

```text
locked section query
locked visual-lock query
locked world-rule query
locked production-rule query
open protected TBD query
canon revision identifiers
```

Those become the deterministic context source for executable skills.
