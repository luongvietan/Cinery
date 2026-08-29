# MVP Release Candidate Evidence (2026-08-29)

Status: **MVP RELEASE CANDIDATE** (automated gates + production bundle pass; clean-install pending)

This document records automated release verification only. The clean-install
smoke test is a separate manual gate; until it passes with recorded evidence,
the release status remains MVP RELEASE CANDIDATE and must not be promoted to
MVP IMPLEMENTED.

## Tool versions

| tool | version |
| --- | --- |
| git commit | 6e77579f95ba236336274e704860761b1c98c184 |
| dirty files at verification | 4 |
| node | v24.14.1 |
| pnpm | 9.12.3 |
| rustc | rustc 1.98.0 (88d9e12ae 2026-08-18) |
| cargo | cargo 1.98.0 (797e8a9bc 2026-08-05) |
| started at | 2026-08-29T13:36:17.8368258+07:00 |

## Automated gates

| gate | exit code | duration |
| --- | --- | --- |
| pnpm-install | 0 | 0.7s |
| pnpm-test | 0 | 10.1s |
| frontend-prereq-build | 0 | 5.5s |
| cargo-test | 0 | 99.7s |
| frontend-build | 0 | 4.8s |
| tauri-build | 0 | 87.7s |
| git-diff-check | 0 | 0s |
| docs-status-check | 0 | 0.1s |

## Bundle artifacts

| absolute path | size (bytes) | sha-256 |
| --- | --- | --- |
| `C:\Users\admin\Desktop\Cinery\apps\desktop\src-tauri\target\release\bundle\msi\AI Cinematic Production OS_0.0.0_x64_en-US.msi` | 6901760 | 8337567fc4544eaa42fbf001ee7e7ed9162f6965d21356797501a6699039a8b3 |
| `C:\Users\admin\Desktop\Cinery\apps\desktop\src-tauri\target\release\bundle\nsis\AI Cinematic Production OS_0.0.0_x64-setup.exe` | 4977753 | adb4232b4dd7994a5d67327f9f8aba1c54f2865fc361d2042ac423bfa3c90e70 |

## Clean-install gate (MANUAL - NOT PERFORMED)

- [ ] clean machine or clean OS user profile used
- [ ] installer hash verified
- [ ] install succeeded from the packaged bundle
- [ ] first launch from installed shortcut succeeded
- [ ] project created, provider configured via OS keychain
- [ ] deterministic mock workflow executed end to end
- [ ] project closed and reopened with state intact
- [ ] diagnostics exported and free of media/secrets
- [ ] application uninstalled without deleting user project data

Tester: _pending_
OS/build: _pending_
Timestamps: _pending_
Result: **NOT PERFORMED**
