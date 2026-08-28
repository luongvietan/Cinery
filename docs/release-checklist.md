# Release Checklist

**MVP IMPLEMENTED** process. Every release must pass all steps below from a
clean checkout. POST-MVP features must not be advertised as available.

## 1. Clean environment

- [ ] Fresh clone of the repository.
- [ ] `pnpm install` completes without errors (fresh dependency install).

## 2. Automated verification

- [ ] TypeScript build: `pnpm --filter @cinematic/desktop build`.
- [ ] All Vitest tests: `pnpm test` (domain + desktop suites).
- [ ] All Rust tests: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -j 1`.
- [ ] Integration tests included in the run above.
- [ ] Acceptance fixture: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -j 1 --test mvp_golden_path`.
- [ ] `git diff --check` is clean (no whitespace errors).

## 3. Production build

- [ ] Tauri production build:
      `pnpm --filter @cinematic/desktop tauri build`.
- [ ] Windows installer/bundle produced by the repository's bundle config
      (`tauri.conf.json` → `bundle.targets`).

## 4. Clean-install smoke test (manual, on a machine without dev tooling)

- [ ] Install the produced bundle.
- [ ] Launch the installed app.
- [ ] Create a new project.
- [ ] Close and reopen the project.
- [ ] Provider configuration screen opens (Providers route).
- [ ] Mock golden workflow executes (Character Builder → Review → mock
      provider → QA).
- [ ] An existing project opens with its state intact.
- [ ] Diagnostics export works (Diagnostics route → Export diagnostics
      bundle) and contains no secrets or media.
- [ ] Uninstalling the app does not delete project directories (projects
      live outside the install prefix, in user-chosen folders).

## 5. Documentation

- [ ] `README.md` matches the current feature set.
- [ ] `docs/architecture.md`, `docs/project-format.md`, `docs/recovery.md`,
      `docs/privacy.md` are current.
- [ ] Documentation distinguishes MVP IMPLEMENTED from POST-MVP; no
      post-MVP roadmap feature is described as available.

## 6. Handoff

- [ ] Tag the release commit.
- [ ] Record the bundle artifact paths and versions in the release notes.
