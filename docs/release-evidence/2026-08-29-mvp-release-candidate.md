# MVP Release Candidate Evidence (2026-08-29)

Status: **MVP RELEASE CANDIDATE** (automated gates and production bundle pass; clean-install pending)

This document records automated qualification of an isolated checkout only. The
clean-install smoke test is a separate manual product gate. Until that test
passes on a clean Windows machine, VM, or OS profile with recorded evidence,
the release must not be labeled `MVP IMPLEMENTED`.

## Release source

| field | value |
| --- | --- |
| release source commit | `b8d22deea721d95854020f48c7ff93edf2265b16` |
| verification worktree | `C:\Users\admin\Desktop\Cinery\.worktrees\mvp-release-verify-2` |
| verification started | `2026-08-29T09:10:30.5485130+07:00` |
| verification completed | `2026-08-29T09:20:30.8548195+07:00` |
| initial tracked/untracked status entries | `0` |
| initial `node_modules` present | `no` |
| initial desktop `dist` present | `no` |
| initial Tauri `target` present | `no` |
| application version | `0.0.0` |

The worktree was created detached directly from the release source commit. It
did not use uncommitted files, `node_modules`, frontend output, Rust target
output, or installer artifacts from the original development checkout.

## Tool versions

| tool | version |
| --- | --- |
| node | `v24.14.1` |
| pnpm | `9.12.3` |
| rustc | `rustc 1.98.0 (88d9e12ae 2026-08-18)` |
| cargo | `cargo 1.98.0 (797e8a9bc 2026-08-05)` |

## Automated gates

| gate | exact command | result | duration | test count / evidence |
| --- | --- | --- | --- | --- |
| dependency install | `pnpm install --frozen-lockfile` | PASS (exit 0) | 2.2s | lockfile frozen; 192 packages installed from clean worktree |
| TypeScript/React tests | `pnpm test` | PASS (exit 0) | 6.4s | 143 passed, 0 failed |
| clean Cargo prerequisite | `pnpm --filter @cinematic/desktop build` | PASS (exit 0) | 4.3s | creates the ignored `frontendDist` required by `tauri::generate_context!()` |
| Rust tests | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -j 1` | PASS (exit 0) | 350.0s | 303 passed, 0 failed |
| frontend production build | `pnpm --filter @cinematic/desktop build` | PASS (exit 0) | 4.2s | TypeScript compile and Vite production build passed |
| Tauri production bundle | `pnpm --filter @cinematic/desktop tauri build` | PASS (exit 0) | 131.2s | MSI and NSIS installers produced |
| whitespace integrity | `git diff --check` | PASS (exit 0) | 0.1s | no whitespace errors |
| documentation state | release script `docs-status-check` | PASS (exit 0) | 0.1s | README and architecture remain `MVP RELEASE CANDIDATE` |

The `clean Cargo prerequisite` is intentionally explicit. A first clean-run
attempt proved that Cargo compilation cannot evaluate Tauri's generated
context while `apps/desktop/dist` is absent. Release source commit `b8d22de`
corrects the qualification script so Rust tests cannot accidentally depend on
stale frontend output from a developer checkout.

## Production bundle artifacts

| absolute path | size (bytes) | SHA-256 |
| --- | ---: | --- |
| `C:\Users\admin\Desktop\Cinery\.worktrees\mvp-release-verify-2\apps\desktop\src-tauri\target\release\bundle\msi\AI Cinematic Production OS_0.0.0_x64_en-US.msi` | 6,602,752 | `71839bc8903ee474ca41aeaf77a4d5d9491df270490f9f75baaa9ae6420bcbd1` |
| `C:\Users\admin\Desktop\Cinery\.worktrees\mvp-release-verify-2\apps\desktop\src-tauri\target\release\bundle\nsis\AI Cinematic Production OS_0.0.0_x64-setup.exe` | 4,737,329 | `4cb203437c60af31e50b313ba2e487e0caec13f53f1df8a18f4602a6c3b75a5e` |

## Credential sentinel scan

Both final installers were scanned as binary data for the deterministic
credential sentinel used by provider keychain acceptance tests.

| artifact | result |
| --- | --- |
| MSI | PASS — sentinel absent |
| NSIS installer | PASS — sentinel absent |

Provider privacy tests also passed as part of the 303-test Rust suite.

## Clean-install gate — NOT PERFORMED

- [ ] clean machine, VM, or clean OS user profile used
- [ ] installer hash independently verified
- [ ] install succeeded from the packaged bundle
- [ ] first launch from the installed shortcut succeeded
- [ ] project created and provider configured through the OS credential vault
- [ ] deterministic MVP workflow executed end to end
- [ ] project closed and reopened with state intact
- [ ] diagnostics exported and verified free of media and secrets
- [ ] application uninstalled without deleting user project data

Tester: _pending_

OS/build: _pending_

Timestamps: _pending_

Result: **NOT PERFORMED**

Final release state: **MVP RELEASE CANDIDATE**.
