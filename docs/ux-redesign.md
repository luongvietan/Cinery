# Cinery UX Redesign — Research, Audit & Implementation Spec

Date: 2026-08-29 · Implemented: 2026-08-30 · Scope: `apps/desktop` frontend (UI aliases only; no backend domain changes)

**Status: implemented.** All slices shipped; acceptance criteria verified by `pnpm -r test` (141 frontend tests), `cargo test` (320 unit + ~140 integration), `tsc` clean, and a production Vite build. Post-redesign additions on top of the original plan: scene Setup/Shots/Render tabs deep-link from scoped Overview actions; the keyframe review grid reuses `GenerationResults` with `saveActionLabel="Use this keyframe"` and `defaultCanonical` (the pin-approval invariant is enforced by `set_shot_keyframe` requiring a canonical version); CharacterLookPanel blocks steps visually *before* the run fails (outfit needs an approved face, sheet needs an approved outfit).

---

## 1. Current Problems

Findings from a full screen/flow audit, severity-ranked. "Affected users" applies to all three personas (first-time creator, experienced AI creator, returning user) unless noted.

| # | Sev | Problem | Where | Cause | Fix direction |
|---|-----|---------|-------|-------|---------------|
| 1 | P0 | **Building one character spans three tabs.** Writing a character lives in Canon → Characters; generating their face/outfit/sheet lives in *Workflows* (forms) or *Production* (face only); results land in Assets. A first-time user cannot discover this. | Nav IA | Generation entry points grew as separate panels instead of attaching to the entity they serve | Attach face/outfit/sheet generation to the Character in Story → Characters |
| 2 | P0 | **Keyframe results are not reviewable.** "Review done — Pin keyframe" silently promotes the *first* generated candidate as canonical without showing it. Misleading label, no compare, no choice. | SceneShots | Promote step was wired before a results UI existed in that context | Show the candidate grid (GenerationResults) in the shot context; user picks; "Use this keyframe" pins the selected one |
| 3 | P1 | **9 top-level nav items mix creation, output, configuration and support.** Overview, Assets, Canon, Workflows, Production, Worlds, Scenes, AI Services, Diagnostics sit at one level. Configuration is as prominent as the creative journey. | ProjectWorkspace nav | Panels accreted per feature | Group nav: creation (Overview/Story/Worlds/Scenes/Assets), output (Generations), system (AI Services, Support) |
| 4 | P1 | **Generation run view exposes runtime internals.** `character-builder@1.1.0 · character.create_face_lock` heading, raw `stepDefinitionId` step list ("compile", "approval", "execute"), JSON "Context snapshot" open by default, "Ready for explicit execution", "Execute Dry Run", `dry-run-v1` model fallback. | WorkflowRunView | Run view was built as a developer/debug view and never translated | Humanize operation + step names; move raw JSON and IDs into collapsed "Technical details" |
| 5 | P1 | **Hidden prerequisite: no AI service.** Nothing on Overview or in a generation form detects that no AI service is connected. First generation fails with a raw error. | Overview, generation forms | Provider state not surfaced outside ProviderSettings | Overview banner ("Connect an AI service…") when none configured; dry-run state explained with CTA |
| 6 | P1 | **Scene editor is a wall of 8 stacked panels** (title, world, characters, props, TBD, readiness, shots, compile) with no dominant action. Copy leaks internals ("assemble exact immutable visual references", "binds the exact immutable version"). | SceneWorkspace | Sections appended in build order | Three-tab editor (Setup / Shots / Render) with header readiness + one dominant CTA |
| 7 | P2 | **"Production" panel is a stub.** One operation card, a "P5" badge, and an AI Director bar that renders mojibake ("RoutingΓÇ¦") and shows raw operation ids with % scores. | ProductionWorkspace, AiDirectorBar | Iteration scaffolding left in place | Remove panel; move a cleaned-up intent bar to Overview; retarget its readiness actions |
| 8 | P2 | **Workflows panel duplicates generation entry points** and advertises operations it can't run ("Unsupported operation" placeholders). History rows show raw `operationId` + `skillId@version`. | WorkflowWorkspace | Catalog-first rather than outcome-first design | Rename to "Generations": active runs + history first; character forms remain for experts; catalog filtered to runnable operations |
| 9 | P2 | **Promotion terminology is engineering-speak.** "Save as Asset Version", "Explicit promotion", "Make the new version canonical", "Target asset". | GenerationResults, PromoteArtifactDialog | Domain verbs surfaced verbatim | "Save to…", "Set as the approved version", "Save into" |
| 10 | P2 | **Dead-end / weak empty states.** Assets empty state sends users to the removed Production panel; scenes list says "assemble immutable references"; run history says "Start one from Production". | Multiple | Copy written against an older IA | Rewrite against new IA with real CTAs |
| 11 | P2 | **Shot rows are button soup** (Edit / Move up / Move down / Generate / Delete inline, plus a conditional pin button) with heavy inline styles. | SceneShots | No compact action pattern | Compact icon actions with labels, tokens-based styles |
| 12 | P3 | Mojibake ellipses in AiDirectorBar (P1 cosmetic but user-visible). | AiDirectorBar | Encoding corruption | Fix + move component |
| 13 | P3 | Two styling regimes (global token CSS vs inline `style={{}}` in scenes/canon) produce inconsistent spacing/typography. | Scene feature | Feature velocity | Move scene editor chrome to token classes |
| 14 | P3 | Dead expression `{activePreset && activePreset.id !== "llm" ? null : null}` in ProviderSettings. | ProviderSettings | Leftover | Remove |

Not broken (audited and kept): provider settings guided flow (presets, vault hints, advanced disclosure), Overview readiness path + plain-language copy, ActionButton explained-disabling pattern, ExecutionPrivacyBadge, focus management in dialogs, dark-first design system, QA/provenance depth in AssetInspector.

## 2. Research Findings

Full method and sources: two research passes (AI-creative products; NLE/beginner tools). Most load-bearing patterns and the adoption decision:

| Pattern | Observed in | Verdict for Cinery |
|---|---|---|
| Entity-attached generation (references/elements live on the character, not in a separate "tools" tab) | Runway Gen-4 References (1–3 images, @-tags), Midjourney Omni Reference (drag into prompt bin), Kling Elements | **Adopt** — face/outfit/sheet generation moves onto the Character in Story |
| Result-first generation history that persists | Runway asset panel, Midjourney Create page grid | **Adopt** — "Generations" panel = active runs + history, entries open run detail |
| Explicit review-before-commit (source vs program monitors) | Premiere/Resolve monitors; Frame.io version stacks + approval statuses | **Adopt** — candidate grid before pinning keyframes; "approved version" language |
| Approval gates on the spine (script → board → animatic) | Boords/FrameForge pipelines; Frame.io all-approvers gates | **Adopt (already present)** — keep Canon locks, approval step, explicit promotion |
| Task-mode split (Cut page vs Edit page) | DaVinci Resolve | **Adapt** — scene editor gets Setup/Shots/Render tabs instead of one stacked wall |
| Persistent contextual inspector | Figma/Resolve right dock | **Keep** — AssetInspector master-detail already follows it |
| Command/intent bar as universal entry | Linear ⌘K, Arc command bar | **Adapt** — AI Director bar moves to Overview as "tell Cinery what you want to do"; no global palette yet |
| Never-dead-end empty states with one CTA | Canva/Dropbox/Slack empty states | **Adopt** — every empty state names the next action and links it |
| Template-first onboarding | CapCut/Canva | **Reject for now** — no sample content in backend; document as debt |
| Metadata-first organization (smart collections) | Final Cut smart collections | **Reject for now** — entity hierarchy + Assets filters are sufficient at current scale |
| Progressive disclosure of advanced params | Canva/Notion/Figma | **Adopt (already present)** — advanced provider settings stay behind disclosure; run JSON moves under "Technical details" |

## 3. Target Mental Model

    Project
    → Story (who's in it, what happens — locked facts)
    → Characters (their looks: face, outfit, sheet — approved references)
    → Worlds (where scenes happen — backdrop)
    → Scenes (cast + world + props → shots → keyframes)
    → Render (compile + generate video)
    → Generations (everything the AI made, always retrievable)
    → Approve what you love (save to Assets as the approved version)

User-facing terms stay filmmaker vocabulary: Story, Character, World, Scene, Shot, Keyframe, Approved. Backend identifiers (canon, promote, canonical, workflow, operation) remain in code and appear only in "Technical details".

## 4. Information Architecture & Navigation

**Before** (9 flat): Overview · Assets · Canon · Workflows · Production · Worlds · Scenes · AI Services · Diagnostics

**After** — three grouped nav regions, one row, visually separated:

- **Create:** Overview · Story · Worlds · Scenes · Assets
- **Output:** Generations
- **System:** AI Services · Support

Panel id ↔ label map (ids stable; UI aliases only — `panelView.ts` keeps `canon|workflows|providers|diagnostics` ids):

| Panel id | Old label | New label | Change |
|---|---|---|---|
| `overview` | Overview | Overview | + provider banner + AI Director intent bar + activity feed |
| `canon` | Canon | **Story** | + per-character look generation (absorbs Production) |
| `worlds` | Worlds | Worlds | unchanged |
| `scenes` | Scenes | Scenes | 3-tab editor restructure |
| `assets` | Assets | Assets | empty-state fix |
| `workflows` | Workflows | **Generations** | history-first; runnable tools; no dead catalog entries |
| `providers` | AI Services | AI Services | dead expression removed; system group |
| `diagnostics` | Diagnostics | **Support** | system group |
| `production` | Production | **removed** | components reused in Story→Characters; readiness actions retargeted |

Backend `OverviewAction.destination` values ("canon" | "assets" | "production" | "scenes") are **kept**; the frontend maps `production` → Story/Characters with the character preselected (no backend change).

## 5. Core User Flows (after)

- **First launch:** Home explains Cinery in 3 steps → name-only project → Overview shows *Connect an AI service* banner if none → readiness path drives story → characters → world → scenes. (Unchanged core; prerequisite made visible.)
- **Create a character:** Story → Characters → "Add character" → write sections → on the character: **Generate face reference** → results grid → pick → "Save & set as approved" → outfit → sheet. Never leaves the character context.
- **Scene → shot → keyframe:** Scenes → pick scene → **Shots** tab → add shot → "Generate keyframe" → run progress → candidate grid → pick → **"Use this keyframe"** → shot shows pinned.
- **Render:** scene → **Render** tab → readiness blockers or "Compile scene" → "Generate video" → progress/cancel → result in Generations.
- **Failure:** run failed state shows what happened + **Try again** + "Check AI Services" link; Overview banner when nothing is connected; dry-run runs labeled "test run (no AI service)".

## 6. Screen Changes

1. **ProjectWorkspace shell** — grouped nav (create/output/system), labels above, production removed.
2. **ProjectOverview** — "Connect an AI service" banner (when zero providers); AI Director bar relocated here (mojibake fixed, human operation names, navigate-on-accept); keep readiness steps/health.
3. **CanonWorkspace → Story** — heading/copy "Story"; Characters tab gains "Look references" (face/outfit/sheet status + generate + results + approve) with character preselected when arriving from Overview actions.
4. **SceneWorkspace** — header (title, status chips, dominant CTA) + tabs Setup · Shots · Render; token-based chrome.
5. **SceneShots** — keyframe review via GenerationResults with explicit pick + "Use this keyframe"; compact shot actions.
6. **WorkflowRunView** — humanized operation/step names, approval copy ("Approve and generate"), dry-run explained, JSON + event/provider history under collapsed "Technical details".
7. **WorkflowWorkspace → Generations** — history + active runs first; "Generation tools" section listing only character operations (+ external-entry hints removed); run rows humanized.
8. **GenerationResults** — copy: "Save into", action label override for keyframes; target picker kept.
9. **PromoteArtifactDialog** — "Save result", "Set as the approved version (used by scenes)".
10. **AssetList empty state** — points to Story/Characters; **Home** — minor copy truth pass.
11. **ProviderSettings** — dead expression removed.

## 7. Component Changes

- New: `NavGroup` styling (existing GooeyNav + CSS), `ProviderBanner` (overview), `CharacterLookPanel` (story feature; composes existing forms + GenerationResults).
- Moved: `AiDirectorBar` (production → rendered in overview, cleaned).
- Removed: ProductionWorkspace, OperationCatalog dead entries, production hero.
- Reused unchanged: ActionButton, BackButton, GooeyNav, ThinkingIndicator, WorkflowRunView (renovated), GenerationResults (extended), CanonSectionCard, dialogs.

## 8. Terminology Changes

| Old (UI) | New (UI) |
|---|---|
| Canon (nav) | Story |
| Workflows (nav) | Generations |
| Diagnostics (nav) | Support |
| Production (nav) | (removed — Story → Characters) |
| "Workflow run" + `skill@ver · operationId` | Generation name + status |
| "Approve request / Reject request" | "Approve and generate" / "Stop" |
| "Ready for explicit execution" / "Execute provider" | "Ready to generate" / "Generate now" |
| "Execute Dry Run" | "Run test (no AI service)" |
| "Context snapshot" / "Compiled request and prompt" | "What the AI will use" / "Prompt sent to the AI" (under Technical details) |
| "Save as Asset Version" | "Save to Assets" (keyframes: "Use this keyframe") |
| "Make the new version canonical" | "Set as the approved version" |
| "Target asset" | "Save into" |
| "Candidate set" | "Results" |

## 9. Migration Strategy

Pure frontend. Panel ids unchanged where possible; `production` id removed from `PanelView` and nav; `OverviewAction.destination === "production"` mapped at the shell. Feature apis untouched. Tests updated with the copy/IA (StatefulTauriFacade unchanged — no new commands).

## 10. Acceptance Criteria

- [x] A new user reaches a generated face reference from the Character editor without visiting a second panel
- [x] Overview shows an actionable banner when no AI service is connected
- [x] Keyframe pinning always shows the candidates and requires an explicit choice
- [x] No raw operation ids, skill versions, step definition ids, or `dry_run` strings visible outside "Technical details"
- [x] Nav groups: 5 create / 1 output / 2 system; Production panel gone; no mojibake
- [x] Scene editor: Setup · Shots · Render tabs; dominant CTA visible per state
- [x] Every empty state has a real CTA; empty-state copy matches the new IA
- [x] `pnpm -r test`, `cargo test`, `tsc` build, production build all pass

## 11. Slices

1. Shell + nav regroup + labels + Production removal/retarget + Support/Diagnostics rename
2. Story/Characters: look-reference generation (absorbs Production + forms), Overview banner + intent bar
3. Run view humanization + Generations panel rework + results copy
4. Scene editor tabs + shot keyframe review flow + shot actions
5. Empty states, PromoteDialog copy, provider dead expression, mojibake, accessibility pass
6. Verification + second audit
