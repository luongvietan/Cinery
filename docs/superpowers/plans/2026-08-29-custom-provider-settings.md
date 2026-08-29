# Custom Provider Settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add project-scoped custom provider settings with independent credentials, models, and headers for LLM, image, and video services.

**Architecture:** Store non-secret provider metadata in a dedicated SQLite table and store API-key/header secrets through the existing CredentialStore. Add typed Tauri commands and a Provider Settings editor while preserving built-in provider behavior and adapter boundaries.

**Tech Stack:** Rust, rusqlite, Tauri commands, React/TypeScript, Vitest/Testing Library.

**Spec:** `docs/superpowers/specs/2026-08-29-custom-provider-settings-design.md`

## Global Constraints

- Provider IDs must match `^[a-z0-9_-]+$`.
- API keys and header values must never be serialized or returned to the frontend.
- Existing built-in providers and OpenAI reference-image behavior must remain unchanged.
- Work only in `codex/mvp-release-source`; preserve the dirty `master` checkout.

---

### Task 1: Backend custom-provider contract and persistence

**Files:**
- Modify: `apps/desktop/src-tauri/src/providers/model.rs`
- Modify: `apps/desktop/src-tauri/src/providers/repository.rs`
- Modify: `apps/desktop/src-tauri/src/db/migrations.rs` (or the existing migration module)
- Test: provider module tests

**Interfaces:**
- Produce `CustomProviderModel`, `CustomProviderHeader`, `CustomProviderDefinition` DTOs.
- Produce repository functions to list/upsert/delete project custom providers.

- [ ] **Step 1: Write failing tests** for ID/URL/name validation, duplicate models/headers, and SQLite round-trip.
- [ ] **Step 2: Run the focused Rust tests** and verify they fail for missing types/functions.
- [ ] **Step 3: Add the migration, DTOs, validation, and repository queries** with secret-free rows.
- [ ] **Step 4: Run focused Rust tests** and verify they pass.
- [ ] **Step 5: Commit** `feat: persist custom provider definitions`.

### Task 2: Tauri commands and credential slots

**Files:**
- Modify: `apps/desktop/src-tauri/src/providers/commands.rs`
- Modify: `apps/desktop/src-tauri/src/providers/service.rs`
- Modify: Tauri command registration module
- Test: command/service tests

**Interfaces:**
- `list_custom_providers(project_root_path: String) -> Result<Vec<CustomProviderDefinition>, AppCommandError>`
- `upsert_custom_provider(project_root_path: String, definition: CustomProviderDefinition) -> Result<CustomProviderDefinition, AppCommandError>`
- `delete_custom_provider(project_root_path: String, provider_id: String) -> Result<(), AppCommandError>`

- [ ] **Step 1: Write failing command tests** for project isolation, built-in delete rejection, and API-key/header redaction.
- [ ] **Step 2: Run focused tests** and verify failure.
- [ ] **Step 3: Implement service/command wiring** and register commands; route secret writes through existing CredentialStore namespaces.
- [ ] **Step 4: Run focused and existing provider tests** and verify pass.
- [ ] **Step 5: Commit** `feat: expose custom provider IPC`.

### Task 3: Frontend API and Provider Settings editor

**Files:**
- Modify: `apps/desktop/src/features/providers/ProviderSettings.tsx`
- Modify: `apps/desktop/src/features/providers/ProviderModelFields.tsx`
- Modify: frontend Tauri API/types module
- Test: `apps/desktop/src/features/providers/ProviderSettings.test.tsx`

**Interfaces:**
- Typed client methods for listing/upserting/deleting custom definitions.
- Form state for repeatable models and headers; API key remains write-only.

- [ ] **Step 1: Add failing UI tests** for custom fields, add/remove rows, validation copy, and save payload.
- [ ] **Step 2: Run the focused Vitest suite** and verify failure.
- [ ] **Step 3: Implement typed API and editor UI** with built-in/custom merged discovery and configured-only credential status.
- [ ] **Step 4: Run focused and full frontend tests** and verify pass.
- [ ] **Step 5: Commit** `feat: add custom provider settings UI`.

### Task 4: Release verification and evidence

**Files:**
- Modify: `docs/release-evidence/clean-install-template.md`
- Modify: `docs/release-evidence/2026-08-29-mvp-release-candidate.md`

- [ ] **Step 1: Run Rust tests, frontend tests, frontend build, Tauri bundle, diff check, and bundle sentinel scan.**
- [ ] **Step 2: Install the rebuilt bundle under the clean standard-user profile and verify custom provider creation, restart persistence, and secret redaction.**
- [ ] **Step 3: Record PASS/FAIL evidence without changing `MVP RELEASE CANDIDATE` unless every manual gate passes.**
- [ ] **Step 4: Commit** `test: qualify custom provider release evidence`.
