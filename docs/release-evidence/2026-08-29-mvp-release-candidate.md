# MVP Release Candidate Evidence (2026-08-29)

Status: **MVP RELEASE CANDIDATE** (automated gates + production bundle pass; clean-install pending)

This document records automated release verification only. The clean-install
smoke test is a separate manual gate; until it passes with recorded evidence,
the release status remains MVP RELEASE CANDIDATE and must not be promoted to
MVP IMPLEMENTED.

## Tool versions

| tool | version |
| --- | --- |
| git commit | d7077b4c6f9d1871ace0862fb85500decdfa4ce4 |
| dirty files at verification | 42 |
| node | v24.14.1 |
| pnpm | 9.12.3 |
| rustc | rustc 1.98.0 (88d9e12ae 2026-08-18) |
| cargo | cargo 1.98.0 (797e8a9bc 2026-08-05) |
| started at | 2026-08-29T08:31:48.4046958+07:00 |

## Automated gates

| gate | exit code | duration |
| --- | --- | --- |
| pnpm-test | 0 | 5.6s |
| cargo-test | 0 | 79.2s |
| frontend-build | 0 | 4.1s |
| tauri-build | 0 | 76.8s |
| git-diff-check | 0 | 0s |
| docs-status-check | 0 | 0s |

## Bundle artifacts

| absolute path | size (bytes) | sha-256 |
| --- | --- | --- |
| `C:\Users\admin\Desktop\Cinery\apps\desktop\src-tauri\target\release\bundle\msi\AI Cinematic Production OS_0.0.0_x64_en-US.msi` | 6631424 | 8c1f1126c1ec816538a2fa35a87881a24294b1ef008fb1ca9c8a45bcc7076503 |
| `C:\Users\admin\Desktop\Cinery\apps\desktop\src-tauri\target\release\bundle\nsis\AI Cinematic Production OS_0.0.0_x64-setup.exe` | 4757180 | 7ba93ac798f77d47f4bdafacd8350a28acc9e70ed925ee267380012e99b971a6 |

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
