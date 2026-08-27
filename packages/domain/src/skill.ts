import { z } from "zod";
import {
  ASSET_TYPES,
  ASSET_VERSION_STATUSES,
  type AssetType,
  type AssetVersionStatus,
} from "./asset";
import { CANON_ENTITY_TYPES, type CanonEntityType } from "./canon";

const semverPattern =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;

const nonEmptyString = z.string().min(1);
const assetTypeSchema = z.enum(ASSET_TYPES);
const assetVersionStatusSchema = z.enum(ASSET_VERSION_STATUSES);
const canonEntityTypeSchema = z.enum(CANON_ENTITY_TYPES);

export interface ExpectedOutputDefinition {
  assetType: AssetType;
  mediaType: "image" | "video" | "audio";
  desiredStatus: "candidate";
  ownerEntityInputRef: string | null;
}

export const expectedOutputDefinitionSchema = z
  .object({
    assetType: assetTypeSchema,
    mediaType: z.enum(["image", "video", "audio"]),
    desiredStatus: z.literal("candidate"),
    ownerEntityInputRef: z.string().min(1).nullable(),
  })
  .strict();

export type Prerequisite =
  | {
      type: "canon_entity_exists";
      entityType: CanonEntityType;
      inputRef: string;
    }
  | {
      type: "canon_section_locked";
      entityInputRef: string;
      sectionKey: string;
    }
  | {
      type: "canonical_asset_exists";
      ownerEntityInputRef: string;
      assetType: AssetType;
    }
  | {
      type: "asset_version_status";
      assetVersionInputRef: string;
      status: AssetVersionStatus;
    };

export const prerequisiteSchema = z.discriminatedUnion("type", [
  z
    .object({
      type: z.literal("canon_entity_exists"),
      entityType: canonEntityTypeSchema,
      inputRef: nonEmptyString,
    })
    .strict(),
  z
    .object({
      type: z.literal("canon_section_locked"),
      entityInputRef: nonEmptyString,
      sectionKey: nonEmptyString,
    })
    .strict(),
  z
    .object({
      type: z.literal("canonical_asset_exists"),
      ownerEntityInputRef: nonEmptyString,
      assetType: assetTypeSchema,
    })
    .strict(),
  z
    .object({
      type: z.literal("asset_version_status"),
      assetVersionInputRef: nonEmptyString,
      status: assetVersionStatusSchema,
    })
    .strict(),
]);

export type TbdGuard =
  | { type: "entity_scope"; entityInputRef: string }
  | { type: "section_scope"; entityInputRef: string; sectionKey: string }
  | { type: "project_scope" };

export const tbdGuardSchema = z.discriminatedUnion("type", [
  z
    .object({ type: z.literal("entity_scope"), entityInputRef: nonEmptyString })
    .strict(),
  z
    .object({
      type: z.literal("section_scope"),
      entityInputRef: nonEmptyString,
      sectionKey: nonEmptyString,
    })
    .strict(),
  z.object({ type: z.literal("project_scope") }).strict(),
]);

export interface ValidateInputStep {
  id: string;
  type: "validate_input";
}

export interface ResolveContextStep {
  id: string;
  type: "resolve_context";
  resolverId: string;
}

export interface CompileRequestStep {
  id: string;
  type: "compile_request";
  compilerId: string;
}

export interface ApprovalStep {
  id: string;
  type: "approval";
  title: string;
  description: string;
  approvalArtifactRef: string;
}

export interface ExecuteStep {
  id: string;
  type: "execute";
  executorKind: "dry_run";
  requestArtifactRef: string;
}

export interface CompleteStep {
  id: string;
  type: "complete";
}

export type WorkflowStepDefinition =
  | ValidateInputStep
  | ResolveContextStep
  | CompileRequestStep
  | ApprovalStep
  | ExecuteStep
  | CompleteStep;

const stepId = nonEmptyString;
const validateInputStepSchema = z
  .object({ id: stepId, type: z.literal("validate_input") })
  .strict();
const resolveContextStepSchema = z
  .object({ id: stepId, type: z.literal("resolve_context"), resolverId: nonEmptyString })
  .strict();
const compileRequestStepSchema = z
  .object({ id: stepId, type: z.literal("compile_request"), compilerId: nonEmptyString })
  .strict();
const approvalStepSchema = z
  .object({
    id: stepId,
    type: z.literal("approval"),
    title: nonEmptyString,
    description: nonEmptyString,
    approvalArtifactRef: nonEmptyString,
  })
  .strict();
const executeStepSchema = z
  .object({
    id: stepId,
    type: z.literal("execute"),
    executorKind: z.literal("dry_run"),
    requestArtifactRef: nonEmptyString,
  })
  .strict();
const completeStepSchema = z
  .object({ id: stepId, type: z.literal("complete") })
  .strict();

export const workflowStepDefinitionSchema = z.discriminatedUnion("type", [
  validateInputStepSchema,
  resolveContextStepSchema,
  compileRequestStepSchema,
  approvalStepSchema,
  executeStepSchema,
  completeStepSchema,
]);

export interface SkillOperation {
  id: string;
  name: string;
  description: string;
  intentExamples: string[];
  inputSchemaId: string;
  prerequisites: Prerequisite[];
  tbdGuards: TbdGuard[];
  workflow: WorkflowStepDefinition[];
  expectedOutput: ExpectedOutputDefinition | null;
}

export const skillOperationSchema = z
  .object({
    id: nonEmptyString,
    name: nonEmptyString,
    description: nonEmptyString,
    intentExamples: z.array(nonEmptyString),
    inputSchemaId: nonEmptyString,
    prerequisites: z.array(prerequisiteSchema),
    tbdGuards: z.array(tbdGuardSchema),
    workflow: z.array(workflowStepDefinitionSchema).min(1),
    expectedOutput: expectedOutputDefinitionSchema.nullable(),
  })
  .strict();

export interface SkillDefinition {
  id: string;
  name: string;
  version: string;
  description: string;
  operations: SkillOperation[];
}

export const skillDefinitionSchema = z
  .object({
    id: nonEmptyString,
    name: nonEmptyString,
    version: z.string().regex(semverPattern, "Version must be valid semantic version"),
    description: nonEmptyString,
    operations: z.array(skillOperationSchema).min(1),
  })
  .strict();

export type SkillDefinitionValue = z.infer<typeof skillDefinitionSchema>;
