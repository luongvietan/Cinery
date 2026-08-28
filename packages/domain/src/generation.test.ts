import { describe, expect, it } from "vitest";
import {
  artifactPromotionSchema,
  generatedArtifactSchema,
  generatedArtifactSourceSchema,
  generationResultSetSchema,
} from "./generation";

describe("generation contracts", () => {
  it("accepts a durable image result set and immutable candidate artifact", () => {
    const resultSet = generationResultSetSchema.parse({
      id: "result-1",
      projectId: "project-1",
      workflowRunId: "run-1",
      workflowStepKey: "execute",
      providerAttemptId: "attempt-1",
      mediaKind: "image",
      requestedOutputCount: 4,
      createdAt: "2026-08-28T00:00:00.000Z",
    });
    const artifact = generatedArtifactSchema.parse({
      id: "artifact-1",
      resultSetId: resultSet.id,
      ordinal: 1,
      mediaKind: "image",
      mimeType: "image/png",
      width: 1280,
      height: 1280,
      byteSize: 42,
      sha256: "a".repeat(64),
      storagePath: "generated/run-1/attempt-1/0001.png",
      captureStatus: "available",
      createdAt: resultSet.createdAt,
    });

    expect(artifact.storagePath).not.toMatch(/^[A-Za-z]:|^\//);
    expect(artifact.captureStatus).toBe("available");
  });

  it("models many source versions and one explicit promotion", () => {
    expect(
      generatedArtifactSourceSchema.parse({
        artifactId: "artifact-1",
        assetVersionId: "version-2",
        role: "identity_reference",
        ordinal: 1,
      }),
    ).toMatchObject({ assetVersionId: "version-2" });

    expect(
      artifactPromotionSchema.parse({
        id: "promotion-1",
        artifactId: "artifact-1",
        assetId: "asset-1",
        assetVersionId: "version-3",
        setCanonical: false,
        createdAt: "2026-08-28T00:00:00.000Z",
      }),
    ).toMatchObject({ setCanonical: false });
  });
});
