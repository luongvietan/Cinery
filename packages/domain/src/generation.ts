import { z } from "zod";
import type { AssetType } from "./asset";
import type { SkillOperation } from "./skill";
import type { WorkflowRunDetail } from "./workflow";

export const generationMediaKindSchema = z.literal("image");
export type GenerationMediaKind = z.infer<typeof generationMediaKindSchema>;

export const generationResultSetSchema = z
  .object({
    id: z.string().min(1),
    projectId: z.string().min(1),
    workflowRunId: z.string().min(1),
    workflowStepKey: z.string().min(1),
    providerAttemptId: z.string().min(1),
    mediaKind: generationMediaKindSchema,
    requestedOutputCount: z.number().int().positive(),
    createdAt: z.string().min(1),
  })
  .strict();
export type GenerationResultSet = z.infer<typeof generationResultSetSchema>;

export const generatedArtifactSchema = z
  .object({
    id: z.string().min(1),
    resultSetId: z.string().min(1),
    ordinal: z.number().int().positive(),
    mediaKind: generationMediaKindSchema,
    mimeType: z.enum(["image/png", "image/jpeg", "image/webp"]),
    width: z.number().int().positive().nullable(),
    height: z.number().int().positive().nullable(),
    byteSize: z.number().int().nonnegative(),
    sha256: z.string().regex(/^[a-f0-9]{64}$/),
    storagePath: z.string().min(1).refine((value) => !/^(?:[A-Za-z]:[\\/]|[\\/]{1,2})/.test(value), "storagePath must be project-relative"),
    captureStatus: z.enum(["materializing", "available", "failed"]),
    captureErrorCode: z.string().min(1).nullable().optional(),
    createdAt: z.string().min(1),
  })
  .strict();
export type GeneratedArtifact = z.infer<typeof generatedArtifactSchema>;

export const generatedArtifactSourceSchema = z
  .object({
    artifactId: z.string().min(1),
    assetVersionId: z.string().min(1),
    role: z.string().min(1),
    ordinal: z.number().int().positive(),
  })
  .strict();
export type GeneratedArtifactSource = z.infer<typeof generatedArtifactSourceSchema>;

export const artifactPromotionSchema = z
  .object({
    id: z.string().min(1),
    artifactId: z.string().min(1),
    assetId: z.string().min(1),
    assetVersionId: z.string().min(1),
    setCanonical: z.boolean(),
    createdAt: z.string().min(1),
  })
  .strict();
export type ArtifactPromotion = z.infer<typeof artifactPromotionSchema>;

export interface GeneratedArtifactDetail {
  artifact: GeneratedArtifact;
  lineage: import("./lineage").ArtifactLineage | null;
}

export interface GenerationResultSetDetail {
  resultSet: GenerationResultSet;
  artifacts: GeneratedArtifactDetail[];
}

/**
 * Provider-neutral description of what a completed generation run produced
 * and which asset a candidate may be saved into. Derived from persisted run
 * input plus the skill operation's expected output — never from component
 * state.
 */
export interface GenerationResultContext {
  workflowRunId: string;
  operationId: string;
  expectedAssetType: AssetType;
  ownerEntityId: string | null;
  resultSets: GenerationResultSetDetail[];
}

/**
 * Derives a {@link GenerationResultContext} from a persisted workflow run
 * and the operation definition. Returns `null` when the operation has no
 * promotable generated output (non-generative operations).
 */
export function deriveGenerationResultContext(
  run: WorkflowRunDetail,
  operation: SkillOperation,
): GenerationResultContext | null {
  const expected = operation.expectedOutput;
  if (!expected) return null;
  let input: Record<string, unknown> = {};
  try {
    input = (JSON.parse(run.run.inputJson) as Record<string, unknown>) ?? {};
  } catch {
    input = {};
  }
  const ownerRef = expected.ownerEntityInputRef;
  const ownerValue = ownerRef ? input[ownerRef] : undefined;
  return {
    workflowRunId: run.run.id,
    operationId: run.run.operationId,
    expectedAssetType: expected.assetType,
    ownerEntityId: typeof ownerValue === "string" && ownerValue.trim() !== "" ? ownerValue : null,
    resultSets: [],
  };
}
