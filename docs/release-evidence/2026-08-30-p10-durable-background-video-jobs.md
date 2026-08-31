# 2026-08-30 P10.1 Durable Background Video Jobs — Release Evidence

**Status: MVP RELEASE CANDIDATE (background execution reconciled; automated
gates pass; manual GUI + clean-install gates remain NOT PERFORMED).**

- Branch: `master` (working tree; P10.0 baseline `4909069`)
- Scope: convert long-running provider execution (video-first, all async
  providers) from a blocking Tauri invoke into a durable, resumable,
  in-process background job model. No queue system, no server, no external
  daemon.

## What was blocking (verified before changing)

`advance_workflow_run` held the frontend invoke open for the entire
generation: submit → `ProviderService::finish_submission` (a synchronous
`loop { poll; sleep }` inside the command) → capture → complete. A real
video generation would pin the invoke for minutes. The provider job was
durable, but execution ownership was not background.

## New architecture

```
Tauri invoke (advance_workflow_run)
  └─ validate → create attempt → submit provider → persist ProviderJob
       ├─ submission already resolved (sync adapters: mock, declarative
       │  sync ops) → complete inline exactly as before
       └─ still pending (real async providers) → attempt 'running'
          → audit `provider.execution.background_started` → RETURN

BackgroundJobRunner (per project daemon thread; tick = 500 ms or wake)
  └─ discover pending durable jobs (workflow_step_executions ⋈ provider_jobs)
      → claim (atomic submitted/polling → polling)
      → cancellation_requested? → resolve cancellation (provider.cancel
         when supported; truthful persistence either way)
      → deadline check (submitted_at + PollingSpec.timeout)
      → poll (no DB handle held) → persist progress (changed values only)
      → Succeeded → fetch → capture (idempotent) → attempt succeeded
         → step completed → run completed (terminal CAS)
      → Failed/timeout → attempt failed → run failed (redacted message)
```

SQLite remains the source of truth; every tick derives work from persisted
state, so a restart resumes durable remote jobs with no duplicate
submission and no new attempt.

## Database changes

Migration `0022_background_provider_jobs.sql` (additive, no rebuild):
`provider_jobs.progress_percent`, `provider_jobs.last_polled_at`, and
`provider_jobs.operation` (the provider operation that created the job —
async declarative adapters persist it so a *rehydrated* adapter instance
can poll after a restart; their in-memory job→operation map dies with the
process). No other schema change; the existing
`workflow_step_executions.status` CHECK already modeled every needed state
(`queued/submitted/running/succeeded/failed/cancellation_requested/
cancelled/unknown`) and `provider_jobs.status` values
(`submitted/polling` → terminal `completed/failed/cancelled`) are plain
TEXT.

## Durable state machine

- attempt: `queued → submitted → running → succeeded | failed | cancelled`
  with `cancellation_requested` as the durable cancel hand-off state.
- provider_jobs: `submitted → polling → completed | failed | cancelled`.
- All terminal transitions are compare-and-set
  (`WHERE status NOT IN (terminal)`); terminal states never flip.

## Automated gates (2026-08-30)

| Gate | Result |
| --- | --- |
| `pnpm -r test` | domain 51/51, desktop 151/151 (45 files) — all pass |
| `cargo test` (all targets) | 482 passed / 0 failed (333 lib unit; new `background_video_job_acceptance` 6 tests) |
| `tsc --noEmit` | pass |
| `vite build` (production) | pass |
| `cargo clippy --all-targets` | 0 errors (pre-existing style warnings only) |
| `tauri build` | MSI + NSIS bundles produced |
| `git diff --check` | clean |

## Final review fixes (pre-commit audit)

Three regressions were found and fixed during the final review, before any
commit was created:

1. **Rehydrated adapters could not poll real async providers** (critical).
   `DeclarativeProvider` kept its job→operation map only in memory, so the
   runner's freshly rehydrated adapter (after a restart) returned `Unknown`
   for every real async job (openai-compatible video, Wan, Replicate) and
   the runner failed it. Fix: `ProviderJobRef.operation` is set by async
   submit, persisted in `provider_jobs.operation` (migration 0022), and
   consulted first by `poll`/`fetch_result`. Proven by the new
   `declarative_async_job_resumes_through_a_rehydrated_adapter`
   acceptance test (a REAL declarative provider over loopback HTTP, cache
   cleared to simulate restart) plus two declarative unit tests.
2. **A transient probe-poll failure failed the whole run.** The ownership
   probe treated retryable poll errors (429/503) as fatal, even though the
   durable job was already persisted and the runner would retry. Fix: a
   retryable probe error now means "genuinely async" — the job hands off to
   the runner, which retries on its own cadence.
3. **Inline sync completions left ghost `submitted` job rows.** A sync
   provider completed inline but its `provider_jobs` row stayed
   `submitted` forever (ghost rows in the Jobs panel + runner discovery).
   Fix: `update_attempt_status` terminal-sets the job row (compare-and-set)
   whenever an inline attempt goes terminal. Regression test added in
   `provider_acceptance.rs`.

Also strengthened: the completion-vs-cancel race test now asserts both
sides deterministically (cancel-lands-first wins; a late cancel against a
completed run fails with a typed error and flips nothing), and a retry
double-click test proves exactly one new attempt with the typed guard
error (never a raw SQLite unique-constraint error).

## New acceptance coverage (offline, no network)

`background_video_job_acceptance.rs`:
1. Early return — invoke returns `running` with the durable job already
   persisted; double-advance is a guarded no-op (no duplicate attempt).
2. Durable progress — first tick persists `progress_percent = 50`,
   readable from SQLite without any runner state.
3. Restart recovery — `ProjectService::open` preserves the durable job;
   a fresh runner resumes polling; exactly one attempt; completion through
   the runner; video artifact captured once; review → promote → exact shot
   pin.
4. Cancellation — durable `cancellation_requested`, runner resolves it,
   attempt + job + run terminal `cancelled`, no artifacts; further ticks
   never flip the terminal state.
5. Completion-vs-cancel race — whichever terminal writer lands first wins;
   the state never flips afterwards.
6. Retry — atomic transaction; attempt 2 with a fresh idempotency key and a
   fresh ProviderJob; attempt 1 stays immutable `failed`; retry completes
   through the runner.
7. Multi-job — two concurrent background video jobs progress independently;
   content-dedup collapses them into one candidate video version.
8. Canon mutation during execution — the frozen compiled request is
   byte-identical before and after; lineage references the run snapshot.

Frontend (`WorkflowRunView.background.test.tsx`, `JobsPanel.test.tsx`):
running state shows provider/model/attempt with a cancel that returns
immediately; the view refreshes authoritative state while non-terminal and
stops when the runner completes; JobsPanel lists provider jobs with
provider/model/status/progress/attempt/operation and "Open workflow"
navigation.

## Manual gates

- **MANUAL BACKGROUND-JOB GUI GATE: NOT PERFORMED** (no GUI automation in
  this session). The §57 smoke path (start video → navigate to Story →
  return → still running → cancel → restart → resume) remains to be walked
  by hand.
- **CLEAN INSTALL GATE: NOT PERFORMED.**

## Release recommendation

**MVP RELEASE CANDIDATE** — unchanged from P10.0: the automated evidence is
stronger (background durability proven offline), but the release gates that
require a human (manual GUI walkthrough, clean install) are still open.
