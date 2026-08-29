# Provider Platform Refactor — Implementation Plan

## Audit findings (current state)

**Request lifecycle today:** UI form (`ProviderSettings.tsx`) → `upsert_custom_provider` Tauri command → `CustomProviderDefinition` (purpose: llm/image/video) persisted in `custom_provider_definitions` (SQLite, migrations head = 18) → on execution, `WorkflowRuntime` → `ProviderService::submit_provider_request` → `custom_execution_adapter` (service.rs:931) maps **purpose → hardcoded adapter**: image → `OpenAiImageProvider`, video → `OpenAiVideoProvider` → Bearer-only `HttpTransport` (`post_json(endpoint, bearer, body)`) → OpenAI request/response schema → `finish_submission` polls ≤17× with **no sleep, no timeout, no cancellation** → `ProviderOutput.uri` (data: URI or URL) → `GenerationService::capture_provider_result` (handles data:/http) or `workflow/ingestion.rs::load_output_bytes` (http **only** — base64 outputs fail on the world-plate/repair path).

**OpenAI assumptions found (the coupling to remove):**
1. `openai.rs` — `/images/generations` + `/images/edits`, body `{model,prompt,size:"1024x1024"}`, response `data[].url|b64_json`, Bearer.
2. `openai_video.rs` — `/videos` submit/poll/content, `id` job path, fixed status vocabulary, Bearer.
3. `service.rs:200-205` — validation is always `GET {base}/models` with Bearer (breaks Cloudflare: "The provider returned HTTP 400 from the validation endpoint." is all the user sees).
4. `service.rs:931-999` — purpose→adapter switch is the giant special-case.
5. `http.rs` — transport signature hardcodes Bearer; no query-param auth, no custom headers on the execution path, no arbitrary methods, no status codes surfaced.
6. `error.rs` — `ProviderError{kind,message,diagnostic}`: no statusCode/providerMessage/requestId/operation; wrapped as raw JSON string in `AppError::ProviderExecution` (UI shows JSON).
7. Polling ignores `provider_configurations.request_timeout_seconds/polling_interval_seconds` (persisted but never read); `Unknown` is terminal after one poll.
8. `qa/adapters/multimodal.rs` + `llm.rs` also assume OpenAI chat schema (LLM surface — kept purpose-based, out of scope for media operations; noted as limitation).

## Target architecture

```
CanonicalGenerationRequest (ProviderExecutionRequest, existing)
        ↓
DeclarativeProvider (implements GenerationProvider, existing trait — no runtime changes above it)
        ├── OperationDefinition  (one per image.generate / image.edit / video.generate / video.imageToVideo / validate)
        │     ├── endpoint: method, pathTemplate, requestType (json|multipart), headers, requestMapping, responseMapping
        │     └── job: Option<AsyncJobConfig> (jobIdPath, status{method,pathTemplate,statusPath,completed,failed,progress,error}, outputMapping, polling{intervalMs,timeoutMs})
        ├── AuthConfig (none | bearer | header{name} | query{name}) + custom static headers (vault-backed, existing scheme)
        └── HttpExecutor (new transport: method/url/headers/body{json|multipart|raw}/status/bytes)
        ↓
CanonicalGenerationResult (ProviderResult — existing; base64 → data: URI normalization, ingestion learns data:)
```

Provider presets are **data** (`presets.rs`) compiled into the runtime config; no `if provider == "cloudflare"` anywhere.

## Work packages

1. **`providers/http.rs`** — replace Bearer-only `HttpTransport` with `HttpExecutor::execute(HttpRequest) -> HttpResponse{status, body}`; migrate llm.rs/qa/multimodal.rs/generation service call sites; keep multipart encoder; no-redirect agent preserved for validation probe.
2. **`providers/config.rs`** (new) — declarative config types + safe engine: JSON-path resolution (`data.0.url`, `result.task_id`), `{{canonical}}` template compilation (prompt, negativePrompt, model, width, height, aspectRatio, seed, steps, quality, duration, fps, image, images, referenceImages, strength + `{{size}}` helper; missing → field omitted; **no eval**), URL interpolation (`{model} {accountId} {providerId} {operation} {jobId}`), validation of configs (URL schemes, header names, template shapes).
3. **`providers/declarative.rs`** (new) — `DeclarativeProvider`: operation selection (edit vs generate by task/references), request compile → auth inject → execute → response parse (url/base64/binary) → sync results in-memory; async: job submit → poll loop (interval from job config, deadline, **cancellation token checked between polls**, progress callback) → final output extraction or fetch endpoint; capabilities derived from operations × model capabilities; rich error mapping (error body extraction paths).
4. **`providers/error.rs`** — `ProviderError` gains `statusCode`, `providerMessage`, `requestId`, `operation` (redacted); human-friendly formatting for `AppError::ProviderExecution` instead of raw JSON.
5. **`providers/presets.rs`** (new) — data presets: openai-compatible, cloudflare-workers-ai, pollinations, runware, replicate, fal, alibaba-wan, custom-rest, fake-async-video. Each = auth + operations (+ validate op). Exposed via `list_provider_presets` command.
6. **`providers/model.rs` + `repository.rs` + migration 0019** — `CustomProviderDefinition` gains `account_id`, `auth`, `operations`, `preset_id` (serialized as `definition_json` column); models gain `capabilities`. Read path: `definition_json` present → new model; NULL → **synthesize from legacy purpose** (image → OpenAI-compatible ops; video → OpenAI videos ops; faithful to previous behavior). Write path always persists new format.
7. **`providers/service.rs`** — `custom_execution_adapter` builds `DeclarativeProvider` from definition + vault secrets; builtin "openai" becomes preset-compiled (openai.rs/openai_video.rs deleted); `test_connection` runs the provider's validate operation via auth-config-correct request, extracts provider messages; polling (`finish_submission`) gains interval/timeout/cancellation/progress callback; cancellation registry consulted by `cancel_workflow_execution`.
8. **`providers/fake_async.rs`** — built-in `fake_async_video` provider: submit→jobId→tick polls→video URL (deterministic mp4-magic bytes), proving async submit/poll/complete end-to-end without credentials.
9. **`workflow/ingestion.rs`** — handle `data:` URIs (normalize base64 outputs end-to-end, matching capture path).
10. **Frontend** — domain types for the new schema; `list_provider_presets`; `ProviderSettings` redesigned: **Simple mode** (preset cards → display name / account id / base url / api token / models / Test connection) + **Advanced settings** disclosure (provider id, auth mode, per-operation method/path/request-type/mappings, async polling, headers) + **Custom REST API** guided single-operation editor. `purpose` becomes derived (back-compat for picker + llm surface).
11. **Tests** — Rust matrix covering all 22 spec cases with deterministic fake transports; frontend tests for simple/advanced/custom flows; regression tests for legacy OpenAI-compatible rows.

## Migration & compatibility

- Migration 0019 (append-only, `schema_migrations`): `ALTER TABLE custom_provider_definitions ADD COLUMN definition_json TEXT`. No destructive change; legacy rows lazily synthesize `definition_json` on read and persist it on next save.
- `purpose` column retained (derived + persisted) so `llm.rs` and provider picker keep working.
- Credential scheme untouched: OS keyring, `project:provider` accounts, per-header accounts, `keyring://` references, `env://` legacy migration.

## Acceptance

- Cloudflare Workers AI: preset → account/token/model → validate (POST /{model}, steps:1) → generate (POST /{model} → `result.image` base64 → data: URI → asset import). No OpenAI-shaped request ever sent.
- Existing OpenAI-compatible providers (both builtin `openai` and legacy custom rows) keep working via the same declarative runtime.
- Async video: `fake_async_video` + declarative async tests prove submit/jobId/poll/complete/timeout/cancel.
- `cargo test`, `pnpm -r test`, tsc, vite build, cargo check all green before claiming done.
