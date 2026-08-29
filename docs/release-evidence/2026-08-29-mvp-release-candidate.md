# MVP Release Candidate Evidence (2026-08-29)

Status: **MVP RELEASE CANDIDATE** (automated gates + production bundle pass; clean-install pending)

This document records automated release verification only. The clean-install
smoke test is a separate manual gate; until it passes with recorded evidence,
the release status remains MVP RELEASE CANDIDATE and must not be promoted to
MVP IMPLEMENTED.

## Tool versions

| tool | version |
| --- | --- |
| git commit | 7936a29adf315d59f1bc75b3bc0cb39ff5782d9b |
| dirty files at verification | 49 |
| node | v24.14.1 |
| pnpm | 9.12.3 |
| rustc | rustc 1.98.0 (88d9e12ae 2026-08-18) |
| cargo | cargo 1.98.0 (797e8a9bc 2026-08-05) |
| started at | 2026-08-29T08:23:46.9637288+07:00 |

## Automated gates

| gate | exit code | duration |
| --- | --- | --- |
| pnpm-test | 0 | 5.4s |
| cargo-test | 0 | 73.3s |
| frontend-build | 0 | 4s |
| git-diff-check | 0 | 0.1s |
| docs-status-check | 0 | 0s |

## Bundle artifacts

| absolute path | size (bytes) | sha-256 |
| --- | --- | --- |
| (bundle skipped by flag) | - | - |

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
