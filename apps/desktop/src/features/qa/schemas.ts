import { z } from "zod";

export const qaRunStatusSchema = z.enum([
  "queued",
  "running",
  "succeeded",
  "failed",
  "cancelled",
]);

export const qaOverallStatusSchema = z.enum(["pass", "fail", "needs_review"]);
export const qaCheckStatusSchema = z.enum([
  "pass",
  "fail",
  "uncertain",
  "not_applicable",
]);
export const qaCheckSourceSchema = z.enum([
  "visual_lock",
  "canonical_reference",
  "operation_expectation",
  "artifact_detection",
]);
export const qaCheckTypeSchema = z.enum([
  "identity_similarity",
  "permanent_visual_lock",
  "hair_consistency",
  "skin_register",
  "outfit_piece",
  "accessory_placement",
  "required_element",
  "forbidden_element",
  "background_requirement",
  "composition_requirement",
  "watermark",
  "unexpected_artifact",
]);
export const qaReviewStatusSchema = z.enum([
  "unreviewed",
  "confirmed",
  "overridden_pass",
  "overridden_fail",
]);

export const qaCheckDefinitionSchema = z
  .object({
    id: z.string().min(1).max(240),
    checkType: qaCheckTypeSchema,
    source: qaCheckSourceSchema,
    key: z.string().min(1).max(240),
    label: z.string().min(1).max(240),
    requirement: z.string().min(1).max(2_000),
    validatorHint: z.string().max(2_000).nullable(),
    blocking: z.boolean(),
    referenceAssetVersionIds: z.array(z.string().min(1)).max(32),
  })
  .strict();

export const qaCheckPlanSchema = z
  .object({
    schemaVersion: z.literal(1),
    assetId: z.string().min(1),
    assetVersionId: z.string().min(1),
    ownerEntityId: z.string().min(1).nullable(),
    assetType: z.string().min(1),
    referenceAssetVersionIds: z.array(z.string().min(1)).max(32),
    checks: z.array(qaCheckDefinitionSchema).min(1).max(128),
    createdAt: z.string().min(1),
  })
  .strict();

export const qaCheckResultSchema = z
  .object({
    checkId: z.string().min(1).max(240),
    status: qaCheckStatusSchema,
    confidence: z.number().min(0).max(1).nullable(),
    observed: z.string().max(4_000),
    reason: z.string().max(4_000),
    repairHint: z.string().max(4_000).nullable(),
  })
  .strict();

export const visualQaResultSchema = z
  .object({
    schemaVersion: z.literal(1),
    checks: z.array(qaCheckResultSchema).max(128),
    modelSummary: z.string().max(4_000).nullable(),
  })
  .strict();

export const qaRunRecordSchema = z
  .object({
    id: z.string().min(1),
    projectId: z.string().min(1),
    assetId: z.string().min(1),
    assetVersionId: z.string().min(1),
    workflowRunId: z.string().min(1).nullable(),
    status: qaRunStatusSchema,
    overallStatus: qaOverallStatusSchema.nullable(),
    adapterId: z.string().nullable(),
    adapterVersion: z.string().nullable(),
    modelId: z.string().nullable(),
    executionLocation: z.string().min(1),
    checkPlan: qaCheckPlanSchema,
    contextSnapshot: z.unknown(),
    rawResponseMetadata: z.unknown().nullable(),
    errorCode: z.string().nullable(),
    errorMessage: z.string().nullable(),
    createdAt: z.string().min(1),
    startedAt: z.string().nullable(),
    completedAt: z.string().nullable(),
  })
  .strict();

export const qaCheckRecordSchema = qaCheckResultSchema
  .extend({
    id: z.string().min(1),
    qaRunId: z.string().min(1),
    checkType: qaCheckTypeSchema,
    source: qaCheckSourceSchema,
    requirement: z.unknown(),
    reviewStatus: qaReviewStatusSchema,
    reviewNote: z.string().nullable(),
    reviewedAt: z.string().nullable(),
    createdAt: z.string().min(1),
  })
  .strict();

export const qaRunDetailSchema = z
  .object({
    run: qaRunRecordSchema,
    checks: z.array(qaCheckRecordSchema).max(128),
  })
  .strict();
