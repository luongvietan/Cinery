# Custom Provider Settings Design

## Goal

Allow each project to define provider records for separate LLM, image, and video services, including endpoint, models, optional API key, and optional HTTP headers.

## Contract

`CustomProviderDefinition` contains:

- `provider_id`: non-empty lowercase ASCII letters, digits, `-`, or `_` (`^[a-z0-9_-]+$`), unique per project.
- `display_name`: non-empty human-readable label.
- `base_url`: absolute HTTP(S) URL with a host and without embedded credentials, query, or fragment. Local/private hosts are intentionally allowed for user-configured local adapters such as ComfyUI.
- `purpose`: one of `llm`, `image`, or `video`; schema-14 records migrate as `legacy` until the user explicitly classifies them, preserving their prior discovery behavior.
- `models`: one or more unique `{ id, name }` entries.
- `headers`: zero or more unique header names. Header values are secrets.

Provider metadata is persisted in SQLite. API keys and header values are stored through the existing `CredentialStore`; only a configured flag is exposed to the frontend. The credential namespace is project-scoped and provider-scoped, so three independent provider IDs can hold three independent secrets.

## IPC and UI

Tauri exposes list, upsert, and delete operations for custom providers. Existing credential commands accept custom IDs without special-casing. The Provider Settings screen is custom-provider-only: it shows saved-provider switching and an editable form with purpose, repeatable model/header rows, and a write-only API-key field. Built-ins remain internal for migration and deterministic local tests but are not shown here.

Saved providers expose a `Test connection` action. It resolves credentials from the OS vault and performs `GET {base_url}/models`; it never calls chat, image-generation, or video-generation endpoints. Redirects are not followed, response bodies are not read, and the result reports only status plus a redacted message. A 2xx means the endpoint was reachable and did not reject the credential, not that every provider proves authentication on `/models`.

## Runtime boundary

Custom definitions are configuration and discovery data. They do not imply a universal LLM/video protocol. Concrete adapters remain responsible for provider-specific request formats; the existing OpenAI image adapter remains unchanged.

## Validation and errors

Invalid IDs, URLs, blank names, duplicate model IDs, invalid/duplicate header names, transport-controlled headers, and attempts to delete built-in providers are rejected before writes. Secret values never appear in DTO serialization, logs, diagnostics, or test snapshots. Header vault keys are case-normalized; removed headers are removed from the vault with DB-failure compensation.

## Testing

Rust tests cover validation, persistence/purpose round-trip, schema-14 migration, redirect refusal, missing/invalid credentials, header cleanup, and secret redaction. Frontend tests cover rendering/editing repeatable fields, save payloads, purpose filtering, stale async results, and write-only credential handling.
