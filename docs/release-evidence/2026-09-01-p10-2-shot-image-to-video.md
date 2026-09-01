# P10.2 Shot-Scoped Image-to-Video — Release Evidence

**Status: IMPLEMENTATION COMPLETE / RELEASE CANDIDATE.** Automated gates pass
for the P10.2 hardening pass. Manual GUI and clean-install validation remain
open, so this document does not claim release readiness.

- Branch: `feature/p10-2-shot-i2v`
- Baseline HEAD before hardening: `11b2410`
- Scope: enforce creator-facing and authoritative validation for the
  shot-scoped image-to-video workflow without changing its frozen source,
  durable job, lineage, or promotion architecture.

## Final hardening

- `ShotImageToVideo` disables generation and explains why when the motion
  prompt is blank or duration is outside 0.5–30 seconds. The click handler
  independently rejects these invalid values rather than trusting HTML input
  constraints.
- `WorkflowRuntime` validates supplied `generationParameters` before creating
  a run: typed shape, duration 0.5–30 seconds, FPS 1–120, unsigned seed, and
  positive `WIDTH:HEIGHT` aspect ratio.
- Aspect ratio, FPS, and seed remain supported in the immutable backend request
  contract but are intentionally deferred from the focused Shot UI. This is a
  product-scope ruling, not an unsupported backend capability.

## Automated gates (2026-09-01)

| Gate | Result |
| --- | --- |
| `pnpm --filter @cinematic/desktop test -- ShotImageToVideo.test.tsx` | 14/14 passed |
| `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib shot_i2v_rejects_invalid_generation_parameters_before_creating_a_run` | 1/1 passed |
| `pnpm -r test` | domain 53/53; desktop 167/167; 58 test files passed |
| `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` | all unit, integration, and doc tests passed; run with `CARGO_BUILD_JOBS=1` |
| `pnpm --filter @cinematic/desktop exec tsc --noEmit` | passed |
| `pnpm --filter @cinematic/desktop exec vite build` | passed; existing chunk-size advisory (>500 kB) remains non-fatal |
| `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings` | passed |
| `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --check` | passed |
| `git diff --check` | passed |
| `pnpm --filter @cinematic/desktop tauri build` | passed; MSI and NSIS artifacts produced |

## Production bundle artifacts

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `AI Cinematic Production OS_0.0.0_x64_en-US.msi` | 7,749,632 | `1BB70A63EB9519CDE42171003386A6A280A96FFFF68668A9D0616C2879A670B9` |
| `AI Cinematic Production OS_0.0.0_x64-setup.exe` | 5,714,642 | `0FE11F2F6B2BBEC75BF93579D513FBFEB3C52A9D3904278BD9490DB9707B0509` |

## Open manual gates

- Manual Shot workspace GUI walkthrough: NOT PERFORMED.
- Clean-install validation: NOT PERFORMED.
- The production build emitted Vite's non-fatal chunk-size advisory. It is not
  introduced by this hardening pass and should be handled as separate frontend
  performance work if release policy requires it.
