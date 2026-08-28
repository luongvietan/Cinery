import { z } from "zod";

const nullableHash = z.string().regex(/^[a-f0-9]{64}$/).nullable();

export const artifactLineageSchema = z
  .object({
    artifactId: z.string().min(1),
    workflowRunId: z.string().min(1),
    workflowStepKey: z.string().min(1),
    workflowDefinitionId: z.string().min(1),
    workflowVersion: z.string().min(1),
    skillId: z.string().min(1),
    skillVersion: z.string().min(1),
    compiledExecutionArtifactId: z.string().min(1),
    compiledRequestSha256: z.string().regex(/^[a-f0-9]{64}$/),
    canonSnapshotId: z.string().min(1).nullable(),
    canonSnapshotSha256: nullableHash,
    providerAttemptId: z.string().min(1),
    providerId: z.string().min(1),
    modelId: z.string().min(1),
    sourceAssetVersionIds: z.array(z.string().min(1)),
    createdAt: z.string().min(1),
  })
  .strict();
export type ArtifactLineage = z.infer<typeof artifactLineageSchema>;
