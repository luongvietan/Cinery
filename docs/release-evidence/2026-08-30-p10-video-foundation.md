# 2026-08-30 P10.0 Video Foundation Reconciliation — Release Evidence

**Status: MVP RELEASE CANDIDATE (video foundation reconciled; automated gates
pass; clean-install + live-provider gates remain NOT PERFORMED).**

- Branch: `codex/p10-video-foundation-reconciliation`
- Base: `master @ 14d16a7`
- Scope: P10.0 video vertical slice + stash reconciliation + parallel
  hardening (scene TBD persistence, timeout policy, facade refresh).

## What was broken (verified before fixing)

The shipped `scene.generate_video` pipeline (commit `5080497`) could not
persist anything:

1. `generation_result_sets.media_kind CHECK (= 'image')` — first INSERT failed.
2. `generated_artifacts.media_kind / mime_type` CHECKs — image-only.
3. `AssetService::create_asset("video", ...)` — rejected by the Sprint-1 gate.
4. `asset_versions.mime_type CHECK` — image-only, rejected `video/mp4`.

Supporting defects: `promote_generated_artifact` used the image import path
for video artifacts; provider attempts were marked `succeeded` before
capture (failed captures blocked retry); orphan recovery skipped `*.mp4`;
the frontend zod schemas rejected video payloads; the video UI had no
review/promote/pin surface; the stashed router was missing while the UI
called `route_production_intent`.

## Automated gates (2026-08-30, branch HEAD)

| Gate | Result |
| --- | --- |
| `pnpm -r test` | domain 51/51, desktop 145/145 (43 files) — all pass |
| `cargo test` (all targets) | 475 passed / 0 failed (332 lib unit; incl. new `video_generation_golden_path` 2 tests, 4 new migration tests) |
| `tsc --noEmit` | pass |
| `vite build` (production) | pass |
| `cargo clippy --all-targets` | 0 errors (pre-existing style warnings only) |
| `tauri build` | MSI + NSIS bundles produced (`Cinematic Production OS_0.0.0`) |
| `git diff --check` | clean |

## Manual gates

- **MANUAL VIDEO GUI GATE: NOT PERFORMED** (no GUI automation available in
  this session).
- **VIDEO LIVE PROVIDER GATE: NOT PERFORMED** (no credentials; Alibaba-Wan
  and the openai-compatible video runtime remain **implemented,
  live-unverified**; the offline golden path via `fake_async_video` is fully
  verified).

## Stash reconciliation (stash@{0} preserved, never popped)

| Component | Action |
| --- | --- |
| `0014_video_assets.sql` | Semantic intent ported into migration `0020` (FK-safe rebuild; the stash version would have failed on real projects) |
| Router (`router/*`, `domain/router.ts`) | PORTED (adapted to the declarative provider platform's credential model) |
| `cinema_video.rs` skill | DISCARDED (superseded by `scene.generate_video`) |
| `mock_video.rs` | DISCARDED (superseded by `fake_async_video`) |
| `comfyui.rs` | DISCARDED from P10.0; future declarative preset |
| `cinema_video_acceptance.rs` | Retargeted as `video_generation_golden_path.rs` |
| `character_pipeline_acceptance.rs` | DISCARDED (identical to master's) |
| `services/ai-worker/` | DEFERRED (placeholder, no consumer) |

stash@{0}, stash@{1}, stash@{2} and all backup branches are untouched.
