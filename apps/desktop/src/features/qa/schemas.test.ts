import { describe, expect, it } from "vitest";
import { qaCheckPlanSchema, qaRunRecordSchema, visualQaResultSchema } from "./schemas";

describe("visual QA schemas", () => {
  it("accepts an exact check plan and rejects provider-invented fields", () => {
    const plan = {
      schemaVersion: 1,
      assetId: "asset-1",
      assetVersionId: "version-1",
      ownerEntityId: "character-1",
      assetType: "character_sheet",
      referenceAssetVersionIds: ["face-v1"],
      checks: [
        {
          id: "lock:scar",
          checkType: "permanent_visual_lock",
          source: "visual_lock",
          key: "scar",
          label: "Eyebrow scar",
          requirement: "Scar on character-right eyebrow",
          validatorHint: null,
          blocking: true,
          referenceAssetVersionIds: ["face-v1"],
        },
      ],
      createdAt: "2026-08-28T00:00:00Z",
    };

    expect(qaCheckPlanSchema.parse(plan)).toEqual(plan);
    expect(() =>
      qaCheckPlanSchema.parse({ ...plan, inventedCanonFact: "blue eyes" }),
    ).toThrow();
  });

  it("rejects confidence outside the inclusive zero-to-one range", () => {
    const result = {
      schemaVersion: 1,
      checks: [
        {
          checkId: "lock:scar",
          status: "fail",
          confidence: 1.01,
          observed: "Wrong side",
          reason: "Mismatch",
          repairHint: "Move scar",
        },
      ],
      modelSummary: null,
    };

    expect(() => visualQaResultSchema.parse(result)).toThrow();
    expect(
      visualQaResultSchema.parse({
        ...result,
        checks: [{ ...result.checks[0], confidence: null }],
      }).checks[0].confidence,
    ).toBeNull();
  });

  it("accepts persisted video QA media and typed temporal checks", () => {
    const run = {
      id: "qa-video-1",
      projectId: "project-1",
      assetId: "video-asset",
      assetVersionId: "video-v1",
      mediaKind: "video",
      workflowRunId: "workflow-1",
      status: "succeeded",
      overallStatus: "fail",
      adapterId: "openai",
      adapterVersion: "1",
      modelId: "video-evaluator",
      executionLocation: "cloud:openai",
      checkPlan: {
        schemaVersion: 1,
        assetId: "video-asset",
        assetVersionId: "video-v1",
        ownerEntityId: "scene-1",
        assetType: "video",
        referenceAssetVersionIds: ["keyframe-v1"],
        checks: [{
          id: "video:integrity",
          checkType: "video_integrity",
          source: "artifact_detection",
          key: "integrity",
          label: "Video integrity",
          requirement: "The video decodes continuously.",
          validatorHint: null,
          blocking: true,
          referenceAssetVersionIds: [],
        }],
        createdAt: "2026-09-01T00:00:00Z",
      },
      contextSnapshot: {},
      rawResponseMetadata: null,
      errorCode: null,
      errorMessage: null,
      createdAt: "2026-09-01T00:00:00Z",
      startedAt: "2026-09-01T00:00:01Z",
      completedAt: "2026-09-01T00:00:02Z",
    };

    expect(qaRunRecordSchema.parse(run)).toEqual(run);
  });
});
