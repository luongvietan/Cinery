import { describe, expect, it } from "vitest";
import { artifactLineageSchema } from "./lineage";

describe("artifact lineage contracts", () => {
  it("pins immutable runtime, provider, canon, and source identities", () => {
    const lineage = artifactLineageSchema.parse({
      artifactId: "artifact-1",
      workflowRunId: "run-1",
      workflowStepKey: "execute",
      workflowDefinitionId: "character-builder",
      workflowVersion: "1.0.0",
      skillId: "character-builder",
      skillVersion: "1.0.0",
      compiledExecutionArtifactId: "compiled-1",
      compiledRequestSha256: "b".repeat(64),
      canonSnapshotId: "canon-snapshot-1",
      canonSnapshotSha256: "c".repeat(64),
      providerAttemptId: "attempt-1",
      providerId: "mock",
      modelId: "mock-image-v1",
      sourceAssetVersionIds: ["version-2"],
      createdAt: "2026-08-28T00:00:00.000Z",
    });

    expect(lineage.sourceAssetVersionIds).toEqual(["version-2"]);
    expect(lineage).not.toHaveProperty("apiKey");
    expect(lineage).not.toHaveProperty("authorization");
  });
});
