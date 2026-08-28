import { z } from "zod";

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
