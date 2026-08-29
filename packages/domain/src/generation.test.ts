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


import {
  deriveGenerationResultContext,
  type GenerationResultContext,
} from "./generation";
import type { SkillOperation } from "./skill";
import type { WorkflowRunDetail } from "./workflow";
import type { AssetType } from "./asset";

function operationWith(expectedAssetType: AssetType, ownerRef: string | null): SkillOperation {
  return {
    id: "character.create_outfit",
    name: "Create Outfit",
    description: "desc",
    intentExamples: [],
    inputSchemaId: "create_outfit",
    prerequisites: [],
    tbdGuards: [],
    workflow: [],
    expectedOutput: expectedAssetType
      ? { assetType: expectedAssetType, mediaType: "image", desiredStatus: "candidate", ownerEntityInputRef: ownerRef }
      : null,
  } as SkillOperation;
}

function runDetail(input: Record<string, unknown>, status = "completed"): WorkflowRunDetail {
  return {
    run: {
      id: "run-1",
      projectId: "project-1",
      skillId: "character-builder",
      skillVersion: "1.1.0",
      operationId: "character.create_outfit",
      status,
      inputJson: JSON.stringify(input),
      prerequisiteReportJson: null,
      contextSnapshotJson: null,
      currentStepIndex: 0,
      failureCode: null,
      failureMessage: null,
      createdAt: "2026-08-29T00:00:00Z",
      updatedAt: "2026-08-29T00:00:00Z",
      completedAt: null,
    },
    steps: [],
    events: [],
  };
}

describe("deriveGenerationResultContext", () => {
  it("derives the expected asset type and owner for the outfit operation", () => {
    const context = deriveGenerationResultContext(
      runDetail({ characterEntityId: "mara" }),
      operationWith("outfit", "characterEntityId"),
    );
    expect(context).not.toBeNull();
    expect(context!.operationId).toBe("character.create_outfit");
    expect(context!.workflowRunId).toBe("run-1");
    expect(context!.expectedAssetType).toBe("outfit");
    expect(context!.ownerEntityId).toBe("mara");
    expect(context!.resultSets).toEqual([]);
  });

  it("derives face_lock with an optional owner for the face lock operation", () => {
    const context = deriveGenerationResultContext(
      runDetail({ characterEntityId: "mara" }, "completed"),
      operationWith("face_lock", "characterEntityId"),
    );
    expect(context!.expectedAssetType).toBe("face_lock");
  });

  it("derives character_sheet for the sheet operation", () => {
    const context = deriveGenerationResultContext(
      runDetail({ characterEntityId: "mara" }),
      operationWith("character_sheet", "characterEntityId"),
    );
    expect(context!.expectedAssetType).toBe("character_sheet");
  });

  it("returns null for non-generative operations", () => {
    const context = deriveGenerationResultContext(
      runDetail({}),
      operationWith("outfit", "characterEntityId") && ({ ...operationWith(null as unknown as AssetType, null) }),
    );
    expect(context).toBeNull();
  });

  it("keeps ownerEntityId null when the input has no owner value", () => {
    const context = deriveGenerationResultContext(
      runDetail({}),
      operationWith("outfit", "characterEntityId"),
    );
    expect(context!.ownerEntityId).toBeNull();
  });

  it("serializes stably across reloads", () => {
    const operation = operationWith("outfit", "characterEntityId");
    const first = deriveGenerationResultContext(runDetail({ characterEntityId: "mara" }), operation);
    const second = deriveGenerationResultContext(runDetail({ characterEntityId: "mara" }), operation);
    expect(JSON.parse(JSON.stringify(first))).toEqual(JSON.parse(JSON.stringify(second)));
  });
});
