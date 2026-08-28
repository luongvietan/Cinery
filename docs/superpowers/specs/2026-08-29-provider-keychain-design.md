# Provider Selection, Reference Images, and Keychain Design

## Purpose

Make provider execution explicit and consistent across production workflows, add a real reference-image path using OpenAI GPT Image 2, and move provider secrets from environment-variable references into the operating system credential store.

This spec covers the provider boundary only. Result promotion, Cinema editing, and release validation are specified separately.

## Current State

- `ProviderService` exposes mock, dry-run, OpenAI, and local ComfyUI adapters.
- The OpenAI adapter only posts JSON to `/v1/images/generations` and declares no reference-image support.
- Provider selection is inconsistent: Production hard-codes mock, Workflow forms rely on a project default, and QA has separate selection logic.
- `provider_configurations.credential_reference` currently stores the name of an environment variable.
- Provider settings cannot securely accept and persist the actual secret.

## Required Behavior

### Explicit provider selection

Every executable workflow must resolve one visible `providerId` and `modelId` before approval. The default comes from project configuration, but the launch form shows it and permits a deliberate override. The compiled provider-neutral request remains free of provider-specific fields; provider selection belongs to execution configuration.

The backend remains authoritative for capability checks. A provider that cannot satisfy media type, reference count, aspect ratio, or task requirements is rejected before submission with a structured, recoverable error.

### Credential storage

Introduce a backend `CredentialStore` boundary with these operations:

```rust
trait CredentialStore {
    fn set_secret(&self, account: &str, secret: &str) -> Result<(), AppError>;
    fn get_secret(&self, account: &str) -> Result<Option<String>, AppError>;
    fn delete_secret(&self, account: &str) -> Result<(), AppError>;
}
```

Production uses an OS-backed implementation through a Rust keyring library compatible with the repository's Rust 1.77.2 floor. Tests use an in-memory implementation and never access the developer's real credential store.

The keyring service name is `cinery`. The account key is deterministic and project-scoped: `<project-id>:<provider-id>`. SQLite stores only this opaque account key in `credential_reference`. API responses expose `credentialConfigured`, never the account key or secret.

Provider configuration accepts a secret only at the command boundary. Saving writes keyring first and then persists the opaque reference. If database persistence fails, the service restores the prior keyring value or deletes the newly created entry. Removing a credential clears the database reference first and then deletes the keyring entry; a keyring deletion failure is reported as an orphaned-secret cleanup error while the provider remains disabled. No response claims a configured credential unless both the keyring lookup and database reference agree.

Existing environment-variable references are treated as legacy configuration. When the referenced variable exists, the backend migrates its value into keyring and replaces the database value with the opaque account key. When it does not exist, the provider remains unconfigured and the UI asks for the secret again. New execution never depends on an environment-variable reference.

### OpenAI GPT Image 2

Replace the OpenAI image model default with `gpt-image-2`. The adapter declares image generation, image editing, one or more reference images, and high-fidelity input support.

Submission behavior:

- No reference attachments: POST JSON to `/v1/images/generations`.
- One or more references: POST multipart form data to `/v1/images/edits`.
- Multipart includes `model=gpt-image-2`, the compiled prompt, `input_fidelity=high`, and only the exact references declared by the immutable workflow snapshot.
- Reference files are resolved from exact AssetVersion IDs immediately before submission. Their current hash and media type are verified against stored metadata.
- The adapter has no database access. The execution layer supplies verified, ephemeral media attachments.
- Both remote URL and base64 image responses are normalized. Inline bytes are captured immediately into project-managed generation storage and are never written to logs or provider metadata.

The adapter must not silently drop references or fall back to text-only generation. A missing, corrupt, oversized, or unsupported attachment fails before any paid request.

### Shared UI control

Create one provider/model selection component used by Character workflows and any future image workflow. It loads configured providers, models, capability disclosure, execution location, and credential status. It provides explicit empty, loading, disabled, and error states and never renders a secret after submission.

Provider Settings accepts an API key in a password field, saves it to keyring, clears the field after success, and shows only configured/not configured status. OpenAI remains visible even before credentials are configured.

## Failure and Recovery Rules

- Credential lookup failure is a configuration error, not a provider execution attempt.
- Provider submission failures create no AssetVersion and no generated artifact.
- Paid requests are never retried automatically.
- Retry creates a new immutable provider attempt and re-verifies references.
- Errors and diagnostics redact authorization headers, secrets, inline image bytes, and keyring account identifiers.
- Deleting a project does not delete credentials automatically; explicit provider credential removal is required.

## Test Strategy

- Unit tests for in-memory credential storage and service behavior.
- Keyring integration is exercised through an injectable fake store in normal CI; an ignored/manual platform test verifies the real OS backend.
- Provider contract tests verify endpoint choice, multipart fields, attachment ordering, exact bytes, capability rejection, and URL/base64 normalization.
- Service tests verify legacy environment-reference migration without persisting the secret.
- React tests verify provider/model selection, capability blockers, key entry clearing, and inaccessible secret values.

## Acceptance Criteria

1. A user can configure OpenAI by entering a secret in the app without placing it in project files or environment variables.
2. Face Lock without a reference uses image generation; Outfit and Character Sheet with exact references use image editing.
3. Provider and model are visible before approval and recorded in the immutable execution attempt.
4. A corrupt or unsupported reference blocks before network submission.
5. Project database, workflow snapshots, logs, diagnostics, and API responses contain no provider secret.

## Non-Goals

- Provider cost comparison or automatic provider routing.
- Cloud credential sync.
- Storing media in the credential store.
- Changing Canon or promotion semantics.
