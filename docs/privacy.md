# Privacy

**MVP RELEASE CANDIDATE.** This product is local-first: your projects, canon,
assets, and history live on your disk, and cloud execution happens only when
you explicitly configure a provider and run a workflow.

## What stays local

- Projects (SQLite database + managed media) live entirely on disk.
- Provider credentials never enter project state. They are stored in the
  platform keychain (via the OS), and projects/databases/snapshots carry
  only a credential **reference** (a `keyring://` pointer).
- No telemetry, no analytics, no automatic uploads.

## Disclosure

- Cloud vs local execution is disclosed before generation/QA runs via the
  `ExecutionPrivacyBadge` (`LOCAL` / `CLOUD: <provider>`).
- Provider switching and paid/cloud retries are never silent.

## Redaction

- A central redaction layer (`diagnostics::redaction::DiagnosticsRedactor`
  and `providers::error::redact_secret`) catches authorization headers,
  API keys, bearer tokens, `sk-`/`api-`/`secret-` prefixes, and related
  patterns before anything reaches a log, an error diagnostic, or a
  diagnostics bundle.
- The diagnostics export is redacted and **excludes media by default**
  (only `app-version.json`, `project-summary.json`,
  `database-version.json`, `project-health.json`, `active-jobs.json`,
  `recent-workflows.json`, and `logs.txt`).
- Secrets are never written to the structured local log.

## What leaves the device during cloud execution

Only the minimum needed to run the job: the candidate plus the explicitly
listed canonical reference images, to the provider you configured. This is
stated in the UI before execution.

## Verification

Automated tests in `apps/desktop/src-tauri/tests/privacy_*.rs` assert zero
secret matches in project state and that redaction covers all secret
patterns. `git grep` over the repository shows no committed secrets.
