# P10.3 Shot Video QA — Release Evidence

**Status: IMPLEMENTATION COMPLETE / RELEASE CANDIDATE.** All automated gates
pass, including the immutable golden-path acceptance test. Manual GUI and
clean-install validation remain open, so this document does not claim
release readiness.

- Branch: `feature/p10-3-shot-video-qa`
- Baseline: `e93fac1` (merge: complete P10.2 shot image-to-video)
- Credentials used: none

## Commits (baseline → HEAD)

```
25b3ee0 feat: prove video QA evidence path
980e0be fix: expose production video QA evidence boundary
fa16b82 feat: register video QA workflow
18c6ff9 fix: dispatch video QA contract steps
2953f54 feat: resolve immutable video QA context
bc3130f feat: plan deterministic video QA checks
941cebd fix: distinguish video QA identity versions
74df21c feat: add strict video QA adapters
f25f124 fix: transfer verified video QA references
680322a feat: persist video QA workflow results
7c63848 fix: sanitize video QA failure persistence
7512b60 feat: add shot-local video QA panel
b120015 fix: resolve Video QA provenance for unpromoted candidates
335c327 test: cover video QA immutable golden path
```

`b120015` is a post-review correctness fix, not a new task: code review of
the Task 6 panel found the central golden-path behavior non-functional (see
below), and `335c327` is Task 7's end-to-end acceptance test.

## Evidence architecture

### Temporal evidence (Task 0)

An explicit two-mode boundary for adapter work:

| Evidence mode | Result | Production contract |
| --- | --- | --- |
| `DirectVideo` | Supported | Reads the exact local MP4 bytes, applies the application's existing minimal `ftyp` signature check, and emits deterministic SHA-256, MIME type, and byte length. Requires no codec process or `PATH` lookup. |
| `SampledFrames` | Unsupported | No video decoder is vendored in the desktop package. The boundary returns typed code `VIDEO_QA_EVIDENCE_UNSUPPORTED` rather than searching `PATH` for `ffmpeg`/`ffprobe`. |

Exposed at `cinematic_desktop_lib::video_qa::evidence`. No Cargo dependency
was added: the proof reuses the already-vendored `sha2` dependency and the
application's existing minimal MP4 signature check.

### Provenance resolution (Task 2, hardened by the post-review fix)

`qa::video_context::resolve_video_qa_context` resolves one exact candidate
`AssetVersion`'s full P10.2 generation lineage (workflow run, provider
attempt, compiled request, source keyframe) without ever reading
`Shot.generated_video_asset_version_id` or the Shot's current keyframe pin.

The original implementation (Task 2/5) required an `artifact_promotions`
row to identify the producing artifact. Completion-time import
(`import_scene_video_candidate`) never writes that row — it is written only
by the later, separate "Use for Shot" action — so the real golden path
("run QA on a normal just-completed candidate, decide on promotion
afterward") always failed with `VIDEO_QA_PROVENANCE_UNSUPPORTED`. This was
caught by review before merge, not after.

The fix (`b120015`) resolves an unpromoted candidate by content identity
(`generated_artifacts.sha256`), scoped to the candidate's owning Scene: the
only built-in video provider (`fake_async_video`) is deterministic and
emits byte-identical output for unrelated shots, so content hash alone
cannot disambiguate. The promotion-linked path is unchanged and tried
first. See `docs/superpowers/plans/2026-09-01-p10-3-shot-video-qa.md` and
the `video_qa_context.rs` / `video_qa_workflow.rs` tests for the full
provenance-resolution and disambiguation coverage.

## Migration ruling

One migration only: `0023_video_qa_check_types.sql` widens `qa_checks`'s
`check_type` CHECK constraint (SQLite cannot alter a CHECK in place) to add
the video check vocabulary, as a content-preserving table rebuild. P10.3
reuses the P6 `qa_runs`/`qa_checks` tables and review APIs unchanged
otherwise — no new tables. The post-review provenance fix required **no**
migration: it added a repository query (`list_video_artifacts_by_content`)
and changed resolution control flow only.

## Focused RED / GREEN evidence

| Task | Focused test(s) | Result |
| --- | --- | --- |
| 0 — evidence boundary | `video_qa_evidence_path.rs` | RED: 0/2 (unimplemented boundary) → GREEN: 2/2 |
| 1 — domain/skill contract | registry/runtime unit tests | RED → GREEN; `visual-qa@1.0.0` regressions stayed green |
| 2 — provenance context | `video_qa_context.rs` | RED → GREEN |
| 3 — check planning | `video_qa_planner.rs` | RED → GREEN |
| 4 — adapters/normalization | `video_qa_normalization.rs` | RED → GREEN; image-normalizer regressions stayed green |
| 5 — workflow/persistence | `video_qa_workflow.rs` | RED → GREEN |
| 6 — Shot-local UI | `VideoQaPanel.test.tsx`, `ShotImageToVideo.test.tsx` | RED → GREEN |
| Post-review fix | `video_qa_context.rs` (+2 tests), `video_qa_workflow.rs` (+1 test), `VideoQaPanel.test.tsx` (+1 test) | RED (reproduced `VIDEO_QA_PROVENANCE_UNSUPPORTED` and the stale-approval UI state) → GREEN |
| 7 — golden path | `shot_video_qa_golden_path.rs` | GREEN: project → Scene/Shot → K1 → I2V → V1 → QA (create/approve/mock execute/review) on the unpromoted candidate → explicit promotion → keyframe drift to K2 + unrelated Canon growth → V2 captured and promoted → V1's QA re-read byte-identical |

## Automated gates (2026-09-02)

| Gate | Result |
| --- | --- |
| `pnpm -r test` | domain 53/53; desktop 187/187; 59 test files passed |
| `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` (`CARGO_BUILD_JOBS=1`) | 366 unit tests (364 passed, 2 pre-existing unrelated failures — see below), all 34 integration test binaries passed in full, 0 doc tests |
| `pnpm --filter @cinematic/desktop exec tsc --noEmit` | passed |
| `pnpm --filter @cinematic/desktop exec vite build` | passed; pre-existing chunk-size advisory (>500 kB) remains non-fatal |
| `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings` | passed |
| `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --check` | passed |
| `git diff --check` | passed (no trailing-whitespace/conflict-marker issues) |
| `pnpm --filter @cinematic/desktop tauri build` | passed; MSI and NSIS artifacts produced |

### Pre-existing unrelated `--lib` failures

`skills::registry::tests::builtin_registry_resolves_the_versioned_scene_keyframe_operation`
and `...world_plate_operation` fail on an unrelated step-count assertion
(`left: 5, right: 4`). Confirmed present, identically, on baseline `7512b60`
(pre-fix HEAD) before any P10.3 review-fix or golden-path work — not
introduced or affected by this branch's changes. Every P10.3-specific
suite (`video_qa_*`, `generation_*`, `cinema_*`, `qa_*`,
`shot_image_to_video_golden_path`, `shot_video_qa_golden_path`, and all
other integration binaries) is fully green.

## Production bundle artifacts

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `AI Cinematic Production OS_0.0.0_x64_en-US.msi` | 7,868,416 | `180233734C5E782591C709C191D930B7DCE2B63E14F4C45B5D9D5B38DA46B0CF` |
| `AI Cinematic Production OS_0.0.0_x64-setup.exe` | 5,777,116 | `586E72AE3590C3C9BF80F854F808AAD621BC578922825788541998587D3EB4A8` |

## Open manual gates

- Manual Shot workspace GUI Video QA walkthrough: NOT PERFORMED (consistent
  with every prior P10.x release-evidence record in this repo — this
  requires a human).
- Clean-install validation: NOT PERFORMED / OPEN. Per
  `docs/release-evidence/README.md`, automated evidence never claims
  `cleanInstallPassed: true`.
- Sampled-frame temporal extraction remains unsupported until a decoder is
  explicitly bundled and release-tested (Task 0 ruling, unchanged).
- The production build emits Vite's pre-existing non-fatal chunk-size
  advisory; not introduced by P10.3 and not a release blocker on its own.
