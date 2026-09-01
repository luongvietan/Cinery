# P10.3 Shot Video QA — Release Evidence

**Status: TASK 0 EVIDENCE BOUNDARY PROVEN; P10.3 RELEASE GATES REMAIN OPEN.**

- Branch: `feature/p10-3-shot-video-qa`
- Baseline: `e93fac1e8748ae3a9a75a42e75bfb1c223c9ffdb`
- Task 0 commit: `feat: prove video QA evidence path`
- Credentials used: none

## Temporal evidence architecture ruling

Task 0 proves an explicit two-mode boundary for later adapter work:

| Evidence mode | Task 0 result | Production contract |
| --- | --- | --- |
| `DirectVideo` | Supported | Read the exact local MP4 bytes, validate the existing ISO-BMFF signature, and emit deterministic SHA-256, MIME type, and byte length. This requires no codec process or `PATH` lookup. A later adapter may select this mode only when its declared capabilities explicitly accept direct video. |
| `SampledFrames` | Unsupported | No video decoder or decoder executable is vendored in the desktop package. The boundary returns typed code `VIDEO_QA_EVIDENCE_UNSUPPORTED`; it does not search for `ffmpeg`, `ffprobe`, a system codec CLI, or a developer `PATH`. |

The focused integration test creates its own deterministic MP4 fixture without
calling a system binary. Repeated direct-video preparation returns the same
source hash and metadata. Frame sampling is not silently substituted, and an
unsupported result must not be normalized into successful temporal evidence.

No Cargo dependency was added: the proof reuses the already-vendored `sha2`
dependency and the application's existing MP4 signature validation.

## Focused RED / GREEN evidence

Before the valid RED run, the worktree did not contain the ignored
`apps/desktop/dist` directory required by `tauri::generate_context!()`. Running
`pnpm --filter @cinematic/desktop exec vite build` supplied that normal build
prerequisite. This setup failure is not counted as RED.

| Phase | Command | Result |
| --- | --- | --- |
| RED | `$env:CARGO_BUILD_JOBS='1'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test video_qa_evidence_path` | Expected failure at the unimplemented evidence boundary: 0 passed, 2 failed. |
| GREEN | `$env:CARGO_BUILD_JOBS='1'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test video_qa_evidence_path` | 2 passed, 0 failed. |

## Migration ruling

No migration is required for Task 0. This spike defines execution-boundary
behavior only and introduces no durable Video QA records.

## Gates not claimed by Task 0

- Full P10.3 regression suite: not yet run; later tasks have not been
  implemented.
- Tauri production bundle install/run with Video QA: not performed because no
  Video QA adapter is integrated in Task 0.
- Manual GUI Video QA walkthrough: not performed.
- Clean-install validation: not performed.
- Sampled-frame extraction: unsupported until a decoder is explicitly bundled
  and release-tested.

Accordingly, Task 0 proves that direct-video evidence has a package-compatible
contract and that missing local temporal decoding fails explicitly. It does
not claim the specification's full packaged production-evidence acceptance
gate, which also requires an integrated adapter and installed-app execution.
