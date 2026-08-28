import { describe, expect, it } from "vitest";
import { qaCheckPlanSchema, visualQaResultSchema } from "./schemas";

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
});
