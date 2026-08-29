# Provider Keychain and Reference Images Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Make provider/model selection consistent, store provider credentials in the operating-system credential vault, and add a production OpenAI GPT Image 2 adapter path that genuinely consumes reference images.

**Architecture:** Keep compiled workflow intent provider-neutral. `ProviderService` owns configuration and credential resolution through an injected `CredentialStore`; adapters receive an ephemeral resolved secret and verified reference attachments only at execution time. OpenAI uses JSON `/v1/images/generations` without references and multipart `/v1/images/edits` with references. React consumes one provider/model field component everywhere a generation can be launched.

**Tech Stack:** Rust 1.77.2, Tauri 2, `keyring` 3.6.3 (MSRV 1.75), `ureq`, `serde`, `base64`; React, TypeScript, Vitest, React Testing Library.

**Spec:** `docs/superpowers/specs/2026-08-29-provider-keychain-design.md`

## Global Constraints

- Never persist or serialize raw credentials outside the OS credential vault.
- Use keyring service `cinery` and account `<project-id>:<provider-id>`.
- Preserve provider/model outside the provider-neutral compiled request; resolve them at the execution boundary.
- Never log raw URLs, base64 payloads, authorization headers, or provider response bodies that can contain images.
- Attach only reference files that passed existing project-boundary, MIME, size, and checksum validation.
- Do not silently change provider or model. OpenAI defaults to `gpt-image-2` only when the user has not made a selection.
- Keep deterministic mocks in the default test suite; real network tests remain opt-in.
- Preserve the existing dirty working tree. Before each task, record `git diff -- <target files>`; stage only newly owned hunks. If an existing modified file cannot be safely split, leave it uncommitted and report it instead of committing user work.

---

## File Structure

- Create `apps/desktop/src-tauri/src/providers/credential_store.rs` for the injectable vault boundary and OS implementation.
- Modify `apps/desktop/src-tauri/Cargo.toml` and `Cargo.lock` for target-specific `keyring` 3.6.3 features.
- Modify `apps/desktop/src-tauri/src/providers/{mod,error,repository,service,commands,registry}.rs` for opaque secret references, migration, and execution-time resolution.
- Modify `apps/desktop/src-tauri/src/providers/{model,http,openai}.rs` for verified attachments and multipart image editing.
- Modify `apps/desktop/src-tauri/src/workflow/{execution,runtime}.rs` to resolve references without giving adapters database access.
- Modify `packages/domain/src/{workflow,skill}.ts` and `apps/desktop/src/features/workflows/api.ts` only where DTOs require provider/model fields.
- Create `apps/desktop/src/features/providers/ProviderModelFields.tsx` and its test.
- Modify `ProviderSettings.tsx`, all three character workflow forms, and Production launch controls to reuse the shared selector.
- Extend provider acceptance, privacy, and UI tests.

### Task 1: Introduce a compatible credential-vault boundary

**Files:**
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/Cargo.lock`
- Create: `apps/desktop/src-tauri/src/providers/credential_store.rs`
- Modify: `apps/desktop/src-tauri/src/providers/mod.rs`
- Modify: `apps/desktop/src-tauri/src/providers/error.rs`
- Test: inline tests in `credential_store.rs`

**Interfaces produced:**

```rust
pub trait CredentialStore: Send + Sync {
    fn set_secret(&self, account: &str, secret: &str) -> Result<(), ProviderError>;
    fn get_secret(&self, account: &str) -> Result<Option<String>, ProviderError>;
    fn delete_secret(&self, account: &str) -> Result<(), ProviderError>;
}

pub struct KeyringCredentialStore { service: &'static str }
pub struct MemoryCredentialStore { secrets: Mutex<HashMap<String, String>> }
```

- [x] **Step 1: Write failing in-memory contract tests**

Cover set/get/delete, missing credentials returning `Ok(None)`, account isolation, replacement, and an injected failing store used later for compensation tests.

- [x] **Step 2: Run the focused test and observe RED**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml credential_store -- --nocapture`

Expected: compile failure because `credential_store` and the trait do not exist.

- [x] **Step 3: Add target-specific keyring dependencies and minimal implementations**

```toml
[target.'cfg(target_os = "windows")'.dependencies]
keyring = { version = "3.6.3", default-features = false, features = ["windows-native"] }

[target.'cfg(target_os = "macos")'.dependencies]
keyring = { version = "3.6.3", default-features = false, features = ["apple-native"] }

[target.'cfg(target_os = "linux")'.dependencies]
keyring = { version = "3.6.3", default-features = false, features = ["sync-secret-service", "crypto-rust"] }
```

Map `keyring::Error::NoEntry` to `Ok(None)` and every other error to a redacted `ProviderErrorKind::CredentialStore`; never include the secret in error text.

- [x] **Step 4: Run GREEN and compile every target-independent caller**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml credential_store -- --nocapture`

Expected: all credential-store tests pass on the development OS.

- [x] **Step 5: Commit only owned hunks**

Suggested commit: `feat: store provider credentials in OS keychain`

### Task 2: Replace environment references with opaque keychain references

**Files:**
- Modify: `apps/desktop/src-tauri/src/providers/repository.rs`
- Modify: `apps/desktop/src-tauri/src/providers/service.rs`
- Modify: `apps/desktop/src-tauri/src/providers/commands.rs`
- Modify: `apps/desktop/src-tauri/src/providers/registry.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src/features/providers/ProviderSettings.tsx`
- Test: `apps/desktop/src-tauri/tests/provider_acceptance.rs`
- Test: `apps/desktop/src/features/providers/ProviderSettings.test.tsx`

**Interfaces consumed/produced:**

```rust
pub struct ProviderService<S: CredentialStore> {
    repository: ProviderRepository,
    credentials: Arc<S>,
}

pub struct ProviderStatusDto {
    pub provider_id: String,
    pub configured: bool,
    pub model_id: Option<String>,
}
```

The database stores only `keyring://cinery/<project-id>:<provider-id>`; IPC returns `configured`, never the reference or secret.

- [x] **Step 1: Write failing service and command-boundary tests**

Test save success, vault failure (no DB row), DB failure after vault write (compensating vault delete), removal order (DB reference first, vault second), orphan cleanup error, and serialized command responses containing neither API key nor opaque reference.

- [x] **Step 2: Add a failing legacy migration test**

Seed an existing `env://OPENAI_API_KEY` row. Assert first access migrates it when the environment variable exists; otherwise assert `configured: false` with a re-entry-required status and no panic.

- [x] **Step 3: Run RED**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml provider_acceptance -- --nocapture`

Expected: assertions fail because configuration still resolves environment variables directly.

- [x] **Step 4: Implement transactional compensation and migration**

```rust
let account = format!("{}:{}", project_id, provider_id);
self.credentials.set_secret(&account, api_key)?;
if let Err(db_error) = self.repository.upsert_secret_ref(
    project_id,
    provider_id,
    &format!("keyring://cinery/{account}"),
) {
    let _ = self.credentials.delete_secret(&account);
    return Err(db_error.into());
}
```

Construct `ProviderService` once in Tauri state with `KeyringCredentialStore`; tests inject `MemoryCredentialStore` or a deterministic failing store.

- [x] **Step 5: Update settings UI and prove secrets stay write-only**

The form displays “Configured”/“Not configured”, accepts a replacement key, clears the input after save, and never receives a stored value from IPC.

Run: `pnpm --filter @cinematic/desktop test -- ProviderSettings.test.tsx`

- [x] **Step 6: Run GREEN and commit**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_acceptance -- --nocapture`

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml privacy -- --nocapture`

Suggested commit: `feat: migrate provider configuration to keychain references`

### Task 3: Carry verified reference attachments to provider adapters

**Files:**
- Modify: `apps/desktop/src-tauri/src/providers/model.rs`
- Modify: `apps/desktop/src-tauri/src/workflow/execution.rs`
- Modify: `apps/desktop/src-tauri/src/workflow/runtime.rs`
- Modify: `apps/desktop/src-tauri/src/generation/storage.rs`
- Test: inline provider-model tests
- Test: `apps/desktop/src-tauri/tests/provider_acceptance.rs`

**Interfaces produced:**

```rust
pub struct ProviderReferenceAttachment {
    pub asset_version_id: String,
    pub file_name: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
    pub sha256: String,
}

pub struct ProviderExecutionRequest {
    // existing fields
    pub reference_attachments: Vec<ProviderReferenceAttachment>,
}
```

- [x] **Step 1: Write failing boundary tests**

Assert ordered reference IDs resolve to ordered attachments, wrong-project paths are rejected, unsupported MIME and hash mismatch fail before submission, and serialized diagnostics omit `bytes`.

- [x] **Step 2: Run RED**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml reference_attachment -- --nocapture`

- [x] **Step 3: Implement one execution-boundary resolver**

Resolve project-owned asset versions through existing repositories/storage validation, load bytes immediately before adapter submission, and drop attachments after the request returns. Do not add repository handles to `GenerationProvider`.

- [x] **Step 4: Run GREEN and existing generation tests**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml generation -- --nocapture`

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml provider -- --nocapture`

Suggested commit: `feat: resolve verified provider reference attachments`

### Task 4: Implement GPT Image 2 generation and multipart edit paths

**Files:**
- Modify: `apps/desktop/src-tauri/src/providers/http.rs`
- Modify: `apps/desktop/src-tauri/src/providers/openai.rs`
- Modify: `apps/desktop/src-tauri/src/providers/registry.rs`
- Test: inline tests in `openai.rs` and `http.rs`
- Test: `apps/desktop/src-tauri/tests/provider_acceptance.rs`

**Interfaces produced:**

```rust
pub trait HttpTransport: Send + Sync {
    fn post_json(&self, request: JsonHttpRequest) -> Result<HttpResponse, ProviderError>;
    fn post_multipart(&self, request: MultipartHttpRequest) -> Result<HttpResponse, ProviderError>;
    fn get_bytes(&self, request: ByteHttpRequest) -> Result<HttpResponse, ProviderError>;
}

pub struct MultipartPart {
    pub field_name: String,
    pub file_name: Option<String>,
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}
```

- [x] **Step 1: Write failing request-shape tests using a recording transport**

Without references, assert POST `/v1/images/generations`, JSON body, selected model, prompt, size, and no multipart. With references, assert POST `/v1/images/edits`, one `image[]` part per verified attachment, `input_fidelity=high`, and no base64 text in logs.

- [x] **Step 2: Add failing response-normalization tests**

Cover GPT image base64 output, legacy URL output, malformed base64, empty data array, HTTP auth/rate-limit errors, and MIME validation before artifact ingestion.

- [x] **Step 3: Run RED**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml openai -- --nocapture`

- [x] **Step 4: Implement the minimal dual transport path**

```rust
if request.reference_attachments.is_empty() {
    transport.post_json(generation_request(request, secret))
} else {
    transport.post_multipart(edit_request(
        request,
        secret,
        "high",
    ))
}
```

Expose capabilities `image_generation=true`, `reference_images=true`, and default model `gpt-image-2`. Preserve the explicit user-selected model.

- [x] **Step 5: Run GREEN plus privacy regression**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml openai -- --nocapture`

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml privacy -- --nocapture`

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_acceptance -- --nocapture`

Suggested commit: `feat: add GPT Image 2 reference-image editing`

### Task 5: Make provider/model selection one shared UI contract

**Files:**
- Create: `apps/desktop/src/features/providers/ProviderModelFields.tsx`
- Create: `apps/desktop/src/features/providers/ProviderModelFields.test.tsx`
- Modify: `apps/desktop/src/features/workflows/CreateFaceLockForm.tsx`
- Modify: `apps/desktop/src/features/workflows/CreateOutfitForm.tsx`
- Modify: `apps/desktop/src/features/workflows/CreateCharacterSheetForm.tsx`
- Modify: `apps/desktop/src/features/production/CharacterBuilderOperation.tsx`
- Modify: `apps/desktop/src/features/production/ProductionWorkspace.test.tsx`
- Modify: `apps/desktop/src/styles/app.css`

**Interface produced:**

```ts
export interface ProviderModelSelection {
  providerId: string;
  modelId: string;
}

interface ProviderModelFieldsProps {
  value: ProviderModelSelection;
  mediaType: "image" | "video";
  requiresReferences: boolean;
  onChange(value: ProviderModelSelection): void;
}
```

- [x] **Step 1: Write failing component tests**

Assert OpenAI remains visible when unconfigured, configuration status is explicit, incompatible providers/models are disabled with a reason, defaults are stable, keyboard labels are present, and user selection survives switching Face/Outfit/Sheet operations.

- [x] **Step 2: Run RED**

Run: `pnpm --filter @cinematic/desktop test -- ProviderModelFields.test.tsx ProductionWorkspace.test.tsx`

- [x] **Step 3: Implement the shared controlled component**

Use current semantic tokens and existing field styles. Do not add a separate card around each select. Expose status text via `aria-describedby`; preserve visible focus and reduced-motion behavior.

- [x] **Step 4: Replace duplicated provider/model controls in all launch forms**

Feed the same controlled value into run creation. Never infer a different model inside individual operation forms.

- [x] **Step 5: Run GREEN and frontend type/build checks**

Run: `pnpm --filter @cinematic/desktop test -- ProviderModelFields.test.tsx ProductionWorkspace.test.tsx WorkflowWorkspace.test.tsx`

Run: `pnpm --filter @cinematic/desktop build`

Suggested commit: `feat: unify provider and model selection`

### Task 6: Complete provider privacy and compatibility verification

**Files:**
- Modify: `apps/desktop/src-tauri/tests/privacy_hardening.rs`
- Modify: `apps/desktop/src-tauri/tests/privacy_integration.rs`
- Modify: `apps/desktop/src-tauri/tests/provider_acceptance.rs`
- Modify: `README.md`

- [x] **Step 1: Add a failing recursive secret scan**

Configure a sentinel credential, execute a mocked reference-image run, export diagnostics and inspect SQLite/project files. Assert the sentinel, authorization header, base64 input, and keyring account reference are absent from user-facing payloads and bundles.

- [x] **Step 2: Run RED, implement any missing redaction, then run GREEN**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml privacy provider_acceptance -- --nocapture`

- [x] **Step 3: Update README configuration instructions**

Document the OS credential vault as the normal path, the one-time legacy environment migration, Linux Secret Service prerequisite, and the fact that credentials are never returned to the UI.

- [x] **Step 4: Run the slice gate**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -j 1`

Run: `pnpm test`

Run: `pnpm --filter @cinematic/desktop build`

Run: `git diff --check`

Expected: every test passes, build succeeds, and no whitespace errors remain.

- [x] **Step 5: Commit only verified owned hunks**

Suggested commit: `docs: document secure provider configuration`
