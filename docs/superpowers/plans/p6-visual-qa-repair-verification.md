# P6 Visual QA + Repair Verification

## Automated checks

- `pnpm test`
- `pnpm test:rust`
- `pnpm --filter @cinematic/desktop build`
- `pnpm --filter @cinematic/desktop tauri build --debug` (requires the local Tauri toolchain)
- `git diff --check`

## Manual desktop pass

1. Open a project containing a generated Character Asset Version and expand its Asset Inspector.
2. Click **Run QA**. Confirm the approval disclosure shows the exact candidate, canonical references, provider/model, and `LOCAL` or `CLOUD` location before execution.
3. Confirm the QA panel keeps failed, needs-review, and passed checks visibly separate. Mark an uncertain check as **Confirm failure** and verify the effective overall status updates without rewriting the model status.
4. Click **Repair Failed Checks**. Review the deterministic plan: only failed conditions appear under changes; identity, wardrobe, accessories, pose, framing, background, and other passed traits appear under preserve.
5. Approve the repair. Confirm the source version remains unchanged, a new candidate child is created with a parent link, and the provider/job/plan provenance is visible in history.
6. Confirm exactly one follow-up QA run appears for the child. A passing child can be explicitly promoted through the existing P5 flow; P6 never promotes it automatically.
7. Repeat with an intentionally failing provider and verify the workflow is failed while no child Asset Version is created.
8. Restart the desktop app with a queued/running QA or repair workflow and verify it is recovered as failed/cancelled without re-executing or creating a duplicate child.

## Expected invariants

- Canon revisions and source QA history are immutable.
- Unresolved `uncertain` evidence cannot enter repair.
- Failed repair produces no phantom Asset Version.
- A child QA failure stops the chain; no autonomous repair loop occurs.
- No API key or bearer token is persisted in workflow input, QA records, or repair provenance.
