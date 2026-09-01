import { describe, expect, it } from "vitest";
import { executionRequestSchema } from "./execution";

describe("execution contracts", () => {
  it("accepts a provider-neutral face-lock request", () => {
    const request = executionRequestSchema.parse({
      requestVersion: 1,
      task: "character_face_lock",
      mediaType: "image",
      prompt: "TASK\nCreate a face lock.",
      references: [],
      constraints: [
        { type: "flat_reference_background", value: "18_percent_neutral_gray" },
        { type: "shadowless_lighting", value: true },
      ],
      expectedOutput: {
        assetType: "face_lock",
        mediaType: "image",
        desiredStatus: "candidate",
        ownerEntityInputRef: "characterEntityId",
      },
      provenance: {
        workflowRunId: "run-1",
        skillId: "character-builder",
        skillVersion: "1.0.0",
        operationId: "character.create_face_lock",
      },
    });

    expect(request.task).toBe("character_face_lock");
    expect(request).not.toHaveProperty("provider");
    expect(request).not.toHaveProperty("model");
  });

  it("accepts a shot image-to-video request with source role and parameters", () => {
    const request = executionRequestSchema.parse({
      requestVersion: 1,
      task: "shot_image_to_video",
      mediaType: "video",
      prompt: "move",
      references: [
        {
          type: "asset_version",
          reference: "version-exact",
          description: "Shot source keyframe",
          role: "source_image",
        },
      ],
      constraints: [],
      expectedOutput: {
        assetType: "video",
        mediaType: "video",
        desiredStatus: "candidate",
        ownerEntityInputRef: "sceneId",
      },
      provenance: {
        workflowRunId: "run-1",
        skillId: "scene-builder",
        skillVersion: "1.0.0",
        operationId: "shot.image_to_video",
      },
      generationParameters: { durationSeconds: 4 },
    });

    expect(request.references[0].role).toBe("source_image");
    expect(request.generationParameters?.durationSeconds).toBe(4);
  });

  it("still accepts legacy requests without generationParameters", () => {
    const request = executionRequestSchema.parse({
      requestVersion: 1,
      task: "character_face_lock",
      mediaType: "image",
      prompt: "TASK",
      references: [],
      constraints: [],
      expectedOutput: {
        assetType: "face_lock",
        mediaType: "image",
        desiredStatus: "candidate",
        ownerEntityInputRef: "characterEntityId",
      },
      provenance: {
        workflowRunId: "run-1",
        skillId: "character-builder",
        skillVersion: "1.0.0",
        operationId: "character.create_face_lock",
      },
    });

    expect(request.generationParameters).toBeUndefined();
  });
});
