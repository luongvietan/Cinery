# P10.3 Shot Video QA — Release Evidence

**Status: TASK 0 EVIDENCE BOUNDARY PROVEN; P10.3 RELEASE GATES REMAIN OPEN.**

- Branch: `feature/p10-3-shot-video-qa`
- Baseline: `e93fac1e8748ae3a9a75a42e75bfb1c223c9ffdb`
- Task 0 commits: `feat: prove video QA evidence path`;
  `fix: expose production video QA evidence boundary`
- Credentials used: none

## Temporal evidence architecture ruling

Task 0 proves an explicit two-mode boundary for later adapter work:

| Evidence mode | Task 0 result | Production contract |
| --- | --- | --- |
| `DirectVideo` | Supported | Read the exact local MP4 bytes, apply the application's existing minimal `ftyp` signature check, and emit deterministic SHA-256, MIME type, and byte length. This does not claim full container validation or media decoding. It requires no codec process or `PATH` lookup. A later adapter may select this mode only when its declared capabilities explicitly accept direct video. |
| `SampledFrames` | Unsupported | No video decoder or decoder executable is vendored in the desktop package. Explicit runtime configuration supplies `TemporalDecoderAvailability::Unavailable`, and the boundary returns typed code `VIDEO_QA_EVIDENCE_UNSUPPORTED`; it does not inspect the package or search for `ffmpeg`, `ffprobe`, a system codec CLI, or a developer `PATH`. |

The reusable API is exposed at
`cinematic_desktop_lib::video_qa::evidence` for the later Task 4 adapter. The
focused integration test decodes a checked-in base64 copy of Web Platform
Tests' decodable H.264/AVC fixture
`media-source/mp4/test-v-128k-640x480-30fps-10kfr.mp4`; fixture guards require
movie/track metadata, AVC decoder configuration, fragmented samples, and media
payload. It invokes no system binary. Repeated direct-video preparation returns
the same source hash and metadata. Frame sampling is not silently substituted,
and an unsupported result must not be normalized into successful temporal
evidence.

No Cargo dependency was added: the proof reuses the already-vendored `sha2`
dependency and the application's existing minimal MP4 signature check.

## Focused RED / GREEN evidence

Before the valid RED run, the worktree did not contain the ignored
`apps/desktop/dist` directory required by `tauri::generate_context!()`. Running
`pnpm --filter @cinematic/desktop exec vite build` supplied that normal build
prerequisite. This setup failure is not counted as RED.

| Phase | Command | Result |
| --- | --- | --- |
| RED | `$env:CARGO_BUILD_JOBS='1'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test video_qa_evidence_path` | Expected failure at the unimplemented evidence boundary: 0 passed, 2 failed. |
| GREEN | `$env:CARGO_BUILD_JOBS='1'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test video_qa_evidence_path` | 2 passed, 0 failed. |

Review-fix RED/GREEN used the same focused command. RED compiled the integration
test against the new production module and failed both tests at the module's
intentional `todo!`; GREEN passed 2/2 after implementing the production
boundary.

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
