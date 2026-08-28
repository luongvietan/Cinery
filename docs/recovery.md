# Recovery

**MVP IMPLEMENTED.** The app is designed so an interrupted run never corrupts
project state. When a project reopens after an interruption, the recovery
surface explains exactly what happened, what state is safe, and what the
user can do next.

## Guarantees

- **No phantom assets.** A failed provider execution never creates an output
  AssetVersion. If one is ever detected, recovery reports it as
  `manual_resolution_required` rather than silently fixing it.
- **No silent retries.** Failed cloud/paid executions require an explicit
  user retry action. Nothing is retried in the background.
- **Immutable snapshots.** Workflow input, context, and output snapshots are
  written once and never rewritten.
- **Explicit canonicalization.** QA passing never auto-promotes. Promotion is
  always an explicit user action and is transactional.

## Job classifications

The recovery scan (`get_project_recovery_state`) classifies every incomplete
job:

| Disposition | Meaning | User action |
|---|---|---|
| `nothing_required` | Safe terminal state; nothing to do | none |
| `await_user_retry` | Provider/cloud failure, no output created | explicit retry |
| `inspect_remote_result` | Provider call may still be running remotely | check remote, then fetch |
| `manual_resolution_required` | Broken state (e.g. phantom asset) | manual intervention |

## Where the rules live

- `apps/desktop/src-tauri/src/recovery/service.rs` — classification logic.
- `apps/desktop/src/features/jobs/JobsPanel.tsx` — the recovery surface.
- Integration tests: `apps/desktop/src-tauri/tests/*recovery*.rs`.
