# P9 Integration Stabilization — Release Evidence (2026-08-29)

Status: **MVP RELEASE CANDIDATE** (promoted from RC-pre-integration; MVP IMPLEMENTED still gated on manual GUI + clean-install passes).

## Source

- Branch `codex/p9-mvp-integration-stabilization` (HEAD `58efaa2`) merged into
  `master` as `35bde57` (no-ff; commits 118c628, 02384f8, 7be191b, 58efaa2 preserved).
- Review blockers fixed during review: ambiguous legacy title mapping
  (migration 0017 rewritten, 4 regression tests), pinned-keyframe canonical
  drift (readiness invariant fixed), artifact promotion idempotency for
  content-deduped candidates (migration 0018).

## Automated gates (executed on master, 2026-08-29)

| Gate | Command | Result |
|---|---|---|
| Domain tests | `pnpm -r test` | 51/51 pass |
| Desktop tests | `pnpm -r test` | 132/132 pass |
| Rust tests | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` | 415 pass / 0 fail (incl. 26 migration tests, unified-scene golden path w/ restart + deterministic recompile) |
| Typecheck + frontend build | `tsc && vite build` | pass |
| Rust release + Tauri packaging | `tauri build` | MSI + NSIS produced |
| git diff check | `git diff --check` | clean |

## Packages

- `AI Cinematic Production OS_0.0.0_x64_en-US.msi` — 6,901,760 bytes — sha256 `c3784a535c19954ed850ca387f1fd7e3ecb80bfb0d950b11993e849990a9c931`
- `AI Cinematic Production OS_0.0.0_x64-setup.exe` — 4,969,770 bytes — sha256 `4aa5b7b60b85cdb5412635fbb9c61b318f16d63ff920928a1bd9c0523f4de6a3`

## Manual gates

- MANUAL GUI GATE: NOT PERFORMED (no GUI automation in the build environment).
- CLEAN INSTALL GATE: NOT PERFORMED.
- OPENAI LIVE GATE: NOT PERFORMED (no credentials available; provider remains live-unverified; Mock/DryRun golden paths fully verified).

Per the release rules, MVP IMPLEMENTED may only be recorded after the real
clean-install pass plus a manual GUI walkthrough of the golden path.
