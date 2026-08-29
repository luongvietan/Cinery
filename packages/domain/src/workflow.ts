import { z } from "zod";
import { ASSET_TYPES } from "./asset";
import { CANON_ENTITY_TYPES, type CanonEntityType } from "./canon";
import type { CanonTbd } from "./tbd";
import { prerequisiteSchema, type Prerequisite } from "./skill";

export const WORKFLOW_RUN_STATUSES = [
  "created",
  "running",
  "waiting_for_approval",
  "ready_for_execution",
  "completed",
  "rejected",
  "cancelled",
  "failed",
] as const;
export type WorkflowRunStatus = (typeof WORKFLOW_RUN_STATUSES)[number];

export const WORKFLOW_STEP_STATUSES = [
  "pending",
  "running",
  "waiting",
  "completed",
  "skipped",
  "failed",
] as const;
export type WorkflowStepStatus = (typeof WORKFLOW_STEP_STATUSES)[number];

export const WORKFLOW_EVENT_TYPES = [
  "run_created",
  "run_started",
  "step_started",
  "step_completed",
  "approval_requested",
  "approval_granted",
  "approval_rejected",
  "execution_started",
  "execution_completed",
  "run_completed",
  "run_cancelled",
  "run_failed",
] as const;
export type WorkflowEventType = (typeof WORKFLOW_EVENT_TYPES)[number];

export interface WorkflowCharacterOption {
  id: string;
  name: string;
}

export interface PrerequisiteCheck {
  id: string;
  prerequisite: Prerequisite;
  status: "pass" | "fail";
  message: string;
  resolvedRef: string | null;
}

export interface PrerequisiteReport {
  passed: boolean;
  checks: PrerequisiteCheck[];
}

export interface CanonSnapshotRef {
  entityId: string;
  entityType: CanonEntityType;
  sectionId: string;
  sectionKey: string;
  revision: number;
  status: "locked";
  value: unknown;
}

export interface AssetSnapshotRef {
  assetId: string;
  assetVersionId: string;
  assetType: string;
  versionNumber: number;
  status: "canonical";
  path: string;
}

export type CanonTbdSnapshot = CanonTbd;

export interface WorkflowContextSnapshot {
  snapshotVersion: 1;
  project: { projectId: string };
  skill: { skillId: string; skillVersion: string; operationId: string };
  input: unknown;
  prerequisiteReport: PrerequisiteReport;
  canon: CanonSnapshotRef[];
  assets: AssetSnapshotRef[];
  protectedTbds: CanonTbdSnapshot[];
  resolvedContext: unknown;
  capturedAt: string;
}

const prerequisiteCheckSchema = z
  .object({
    id: z.string().min(1),
    prerequisite: prerequisiteSchema,
    status: z.enum(["pass", "fail"]),
    message: z.string(),
    resolvedRef: z.string().nullable(),
  })
  .strict();

const canonSnapshotRefSchema = z
  .object({
    entityId: z.string().min(1),
    entityType: z.enum(CANON_ENTITY_TYPES),
    sectionId: z.string().min(1),
    sectionKey: z.string().min(1),
    revision: z.number().int().positive(),
    status: z.literal("locked"),
    value: z.unknown(),
  })
  .strict();

const assetSnapshotRefSchema = z
  .object({
    assetId: z.string().min(1),
    assetVersionId: z.string().min(1),
    assetType: z.enum(ASSET_TYPES),
    versionNumber: z.number().int().positive(),
    status: z.literal("canonical"),
    path: z.string().min(1),
  })
  .strict();

const canonTbdSnapshotSchema = z
  .object({
    id: z.string().min(1),
    projectId: z.string().min(1),
    canonEntityId: z.string().min(1).nullable(),
    sectionKey: z.string().min(1).nullable(),
    topic: z.string().min(1),
    note: z.string().nullable(),
    protected: z.boolean(),
    status: z.enum(["open", "resolved"]),
    resolutionText: z.string().nullable(),
    createdAt: z.string().min(1),
    updatedAt: z.string().min(1),
    resolvedAt: z.string().nullable(),
  })
  .strict();

export const workflowContextSnapshotSchema = z
  .object({
    snapshotVersion: z.literal(1),
    project: z.object({ projectId: z.string().min(1) }).strict(),
    skill: z
      .object({
        skillId: z.string().min(1),
        skillVersion: z.string().min(1),
        operationId: z.string().min(1),
      })
      .strict(),
    input: z.unknown(),
    prerequisiteReport: z
      .object({ passed: z.boolean(), checks: z.array(prerequisiteCheckSchema) })
      .strict(),
    canon: z.array(canonSnapshotRefSchema),
    assets: z.array(assetSnapshotRefSchema),
    protectedTbds: z.array(canonTbdSnapshotSchema),
    resolvedContext: z.unknown(),
    capturedAt: z.string().datetime({ offset: true }),
  })
  .strict();

export interface WorkflowRunRecord {
  id: string;
  projectId: string;
  skillId: string;
  skillVersion: string;
  operationId: string;
  status: WorkflowRunStatus;
  inputJson: string;
  prerequisiteReportJson: string | null;
  contextSnapshotJson: string | null;
  currentStepIndex: number;
  failureCode: string | null;
  failureMessage: string | null;
  createdAt: string;
  updatedAt: string;
  completedAt: string | null;
}

export interface WorkflowStepRecord {
  id: string;
  workflowRunId: string;
  stepDefinitionId: string;
  stepIndex: number;
  stepType: string;
  status: WorkflowStepStatus;
  inputJson: string | null;
  outputJson: string | null;
  startedAt: string | null;
  completedAt: string | null;
}

export interface WorkflowEventRecord {
  id: string;
  workflowRunId: string;
  sequence: number;
  eventType: WorkflowEventType;
  stepDefinitionId: string | null;
  payloadJson: string | null;
  createdAt: string;
}

export interface WorkflowRunDetail {
  run: WorkflowRunRecord;
  steps: WorkflowStepRecord[];
  events: WorkflowEventRecord[];
  providerExecutions?: ProviderExecutionSummary[];
}

export type ProviderMediaType = "image" | "video";
export type ProviderLifecycle =
  | "queued"
  | "submitted"
  | "running"
  | "succeeded"
  | "failed"
  | "cancellation_requested"
  | "cancelled"
  | "unknown";

export interface ProviderCapabilities {
  mediaTypes: ProviderMediaType[];
  supportsSeed: boolean;
  supportsNegativePrompt: boolean;
  supportsReferenceImage: boolean;
  supportsImageEdit: boolean;
  supportsMultipleReferenceImages: boolean;
  supportsImageToVideo: boolean;
  supportsCancel: boolean;
  supportsProgress: boolean;
  supportedAspectRatios: string[];
  supportedModels: string[];
}

export interface ProviderConfigurationStatus {
  providerId: string;
  enabled: boolean;
  /** True only when the backend verified both the vault entry and the DB reference. */
  credentialConfigured: boolean;
  /** Stable model list for the provider (e.g. ["gpt-image-2"]). */
  models: string[];
  defaultModel: string | null;
}

export interface CustomProviderModel {
  id: string;
  name: string;
  /** Operations this model supports; empty means "every provider operation". */
  capabilities?: string[];
}

export interface CustomProviderHeader {
  name: string;
  /** Write-only input; omitted from responses. */
  value?: string;
}

// ---------------------------------------------------------------------------
// Declarative provider configuration (provider platform)
// ---------------------------------------------------------------------------

/** How the provider's API key is attached to requests. */
export type ProviderAuthMode = "none" | "bearer" | "header" | "query";

export interface ProviderAuthConfig {
  mode: ProviderAuthMode;
  /** Header or query parameter name for header/query modes. */
  credentialName?: string | null;
}

export type ProviderRequestType = "json" | "multipart" | "form_url_encoded";

export interface ProviderResponseMapping {
  outputsPath?: string | null;
  urlPath?: string | null;
  base64Path?: string | null;
  /** The whole response body is the asset. */
  binaryResponse: boolean;
  mimeType: string;
  filename: string;
  providerRequestIdPath?: string | null;
}

export interface ProviderMultipartField {
  name: string;
  kind: "text" | "file";
  /** Template for text fields, e.g. "{{prompt}}". */
  value?: string | null;
  /** File source: "image" | "images" | "referenceImages". */
  source?: string | null;
}

export interface ProviderStatusEndpoint {
  method: string;
  /** Path template relative to the base URL; {jobId} is interpolated. */
  pathTemplate: string;
  statusPath: string;
  completedValues: string[];
  failedValues: string[];
  progressPath?: string | null;
  errorMessagePath?: string | null;
}

export interface ProviderAsyncJobConfig {
  jobIdPath: string;
  status: ProviderStatusEndpoint;
  output: {
    fetchPathTemplate?: string | null;
    fetchMethod: string;
    response: ProviderResponseMapping;
  };
  polling: { intervalMs: number; timeoutMs: number };
}

/** One configurable operation endpoint (e.g. image.generate). */
export interface ProviderOperationEndpoint {
  method: string;
  pathTemplate: string;
  requestType: ProviderRequestType;
  /** JSON template with {{canonical}} placeholders. */
  requestMapping?: unknown;
  multipartFields?: ProviderMultipartField[];
  /** Extra static headers for this operation. */
  headers?: Record<string, string>;
  response: ProviderResponseMapping;
  /** Present when the operation is asynchronous (submit → poll → fetch). */
  job?: ProviderAsyncJobConfig | null;
}

/** The declarative runtime configuration stored with a provider. */
export interface ProviderRuntimeConfig {
  auth: ProviderAuthConfig;
  accountId?: string | null;
  /** Static (non-secret) headers; secret headers use CustomProviderHeader. */
  headers?: Record<string, string>;
  /** Operations keyed by name: image.generate, image.edit, video.generate, … */
  operations?: Record<string, ProviderOperationEndpoint>;
  errorMapping?: {
    messagePath?: string | null;
    codePath?: string | null;
    requestIdPath?: string | null;
  } | null;
}

/** A SIMPLE-mode preset describing a known provider kind. */
export interface ProviderPreset {
  id: string;
  label: string;
  description: string;
  internal: boolean;
  defaultBaseUrl: string;
  requiresAccountId: boolean;
  auth: ProviderAuthConfig;
  /** [model id, label] pairs offered by default. */
  defaultModels: [string, string][];
  runtime: ProviderRuntimeConfig;
}

export interface CustomProviderDefinition {
  providerId: string;
  displayName: string;
  baseUrl: string;
  purpose: "legacy" | "llm" | "image" | "video";
  /** Preset that generated this definition, when applicable. */
  presetId?: string | null;
  /** Declarative endpoint configuration; secrets are never stored here. */
  runtime?: ProviderRuntimeConfig;
  /** Write-only input; omitted from responses. */
  apiKey?: string;
  /** Non-secret display hint (e.g. "sk-j9ml•••ray") derived from the vault secret. */
  apiKeyHint?: string | null;
  models: CustomProviderModel[];
  headers: CustomProviderHeader[];
}

export interface ProviderConnectionTestResult {
  providerId: string;
  endpoint: string;
  connected: boolean;
  statusCode: number | null;
  message: string;
}

export interface ProviderExecutionSummary {
  id: string;
  stepDefinitionId: string;
  attemptNumber: number;
  providerId: string;
  modelId: string;
  adapterVersion: number;
  status: ProviderLifecycle;
  providerJobId: string | null;
  normalizedErrorJson: string | null;
  startedAt: string;
  completedAt: string | null;
}
