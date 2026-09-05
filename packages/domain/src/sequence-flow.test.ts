import { describe, expect, it } from "vitest";
import {
  extensionDirectionSchema,
  extensionRequestSchema,
  sequenceBriefSchema,
  sequenceFlowSchema,
  sequencePreflightSchema,
  sequenceStageSchema,
} from "./sequence-flow.js";

function compilation() {
  return {
    id: "c1",
    projectId: "p",
    sceneId: "scene-1",
    totalDurationSeconds: 8,
    shots: [
      { order: 0, durationSeconds: 4, intent: "Establish" },
      { order: 1, durationSeconds: 4, intent: "Close" },
    ],
    behavioralLocks: {},
    worldContinuity: {},
    providerPrompt: "CINEMA PROMPT",
    createdAt: "2026-09-05T00:00:00.000Z",
  };
}

function preflight() {
  return {
    sceneId: "scene-1",
    compilation: compilation(),
    providerPrompt: "CINEMA PROMPT",
    references: [{ assetId: "plate-1", versionId: "plate-v2", role: "world_plate" }],
    estimatedCredits: 240,
    runtimeRecommendation: "8s total is within the recommended 15-second prompt unit",
    canGenerate: true,
    blockers: [],
  };
}

describe("sequence brief schema", () => {
  it("accepts a locked brief and rejects an empty creative intent", () => {
    expect(sequenceBriefSchema.parse({ intent: "Tay notices the door", energy: "elevated", creditCap: 800 })).toMatchObject({ creditCap: 800 });
    expect(() => sequenceBriefSchema.parse({ intent: " ", energy: "elevated", creditCap: 800 })).toThrow();
  });

  it("rejects out-of-range target durations", () => {
    expect(() =>
      sequenceBriefSchema.parse({ intent: "Beat", energy: "kinetic", targetDurationSeconds: 0, creditCap: 800 }),
    ).toThrow();
    expect(() =>
      sequenceBriefSchema.parse({ intent: "Beat", energy: "kinetic", targetDurationSeconds: 121, creditCap: 800 }),
    ).toThrow();
    expect(
      sequenceBriefSchema.parse({ intent: "Beat", energy: "kinetic", targetDurationSeconds: 15, creditCap: 800 })
        .targetDurationSeconds,
    ).toBe(15);
  });

  it("rejects invalid energy and credit caps", () => {
    expect(() => sequenceBriefSchema.parse({ intent: "Beat", energy: "whimsical", creditCap: 800 })).toThrow();
    expect(() => sequenceBriefSchema.parse({ intent: "Beat", energy: "composed", creditCap: -1 })).toThrow();
    expect(() => sequenceBriefSchema.parse({ intent: "Beat", energy: "composed", creditCap: 800.5 })).toThrow();
  });
});

describe("extension direction schema", () => {
  it("only accepts the two deliberate extension directions", () => {
    expect(extensionDirectionSchema.parse("prequel")).toBe("prequel");
    expect(() => extensionDirectionSchema.parse("continue")).toThrow();
  });
});

describe("sequence flow schema", () => {
  it("accepts a draft flow and rejects an unknown stage", () => {
    const flow = {
      sceneId: "scene-1",
      brief: { intent: "Tay notices the door", energy: "elevated", creditCap: 800 },
      stage: "draft",
      approvedCompilationId: null,
      canonicalShotId: null,
      extensionDirection: null,
      createdAt: "2026-09-05T00:00:00.000Z",
      updatedAt: "2026-09-05T00:00:00.000Z",
    };
    expect(sequenceFlowSchema.parse(flow).stage).toBe("draft");
    expect(sequenceStageSchema.parse("canonical_selected")).toBe("canonical_selected");
    expect(() => sequenceFlowSchema.parse({ ...flow, stage: "paused" })).toThrow();
  });
});

describe("sequence preflight schema", () => {
  it("accepts a ready preflight with the full compilation", () => {
    expect(sequencePreflightSchema.parse(preflight()).canGenerate).toBe(true);
  });

  it("keeps canGenerate consistent with blockers", () => {
    expect(() =>
      sequencePreflightSchema.parse({
        ...preflight(),
        canGenerate: true,
        blockers: [{ code: "world_reference_missing", message: "Missing scene plate" }],
      }),
    ).toThrow();
    expect(() => sequencePreflightSchema.parse({ ...preflight(), canGenerate: false, blockers: [] })).toThrow();
  });

  it("validates the embedded compilation", () => {
    expect(() =>
      sequencePreflightSchema.parse({ ...preflight(), compilation: { ...compilation(), totalDurationSeconds: 9 } }),
    ).toThrow();
  });
});

describe("extension request schema", () => {
  it("requires an exact canonical video and a deliberate direction", () => {
    const request = {
      sceneId: "scene-1",
      shotId: "shot-1",
      direction: "sequel",
      canonicalVideoAssetVersionId: "video-v4",
      carriedLocks: { movement: "locked walk" },
      worldContinuity: { plateId: "plate-v2" },
      continuationPrompt: "CONTINUATION PROMPT",
    };
    expect(extensionRequestSchema.parse(request).direction).toBe("sequel");
    expect(() => extensionRequestSchema.parse({ ...request, canonicalVideoAssetVersionId: " " })).toThrow();
    expect(() => extensionRequestSchema.parse({ ...request, direction: "continue" })).toThrow();
  });
});
