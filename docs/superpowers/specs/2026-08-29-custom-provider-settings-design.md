# Custom Provider Settings Design

## Goal

Allow each project to define provider records for separate LLM, image, and video services, including endpoint, models, optional API key, and optional HTTP headers.

## Contract

`CustomProviderDefinition` contains:

- `provider_id`: non-empty lowercase ASCII letters, digits, `-`, or `_` (`^[a-z0-9_-]+$`), unique per project.
- `display_name`: non-empty human-readable label.
- `base_url`: absolute HTTP(S) URL without embedded credentials.
- `models`: one or more unique `{ id, name }` entries.
- `headers`: zero or more unique header names. Header values are secrets.

Provider metadata is persisted in SQLite. API keys and header values are stored through the existing `CredentialStore`; only a configured flag is exposed to the frontend. The credential namespace is project-scoped and provider-scoped, so three independent provider IDs can hold three independent secrets.

## IPC and UI

Tauri exposes list, upsert, and delete operations for custom providers. Listing merges built-in providers and project custom providers. Existing credential commands accept custom IDs without special-casing. The Provider Settings screen keeps the existing built-in selector and adds an editable custom-provider section with repeatable model/header rows and a write-only API-key field.

## Runtime boundary

Custom definitions are configuration and discovery data. They do not imply a universal LLM/video protocol. Concrete adapters remain responsible for provider-specific request formats; the existing OpenAI image adapter remains unchanged.

## Validation and errors

Invalid IDs, URLs, blank names, duplicate model IDs, duplicate header names, and attempts to delete built-in providers are rejected before writes. Secret values never appear in DTO serialization, logs, diagnostics, or test snapshots.

## Testing

Rust tests cover validation, persistence round-trip, duplicate rejection, and secret redaction. Frontend tests cover rendering/editing repeatable fields, save payloads, merged discovery, and write-only credential handling.
