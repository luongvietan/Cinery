import { describe, expect, it } from "vitest";
import {
  WORKFLOW_RUN_STATUSES,
  WORKFLOW_STEP_STATUSES,
  workflowContextSnapshotSchema,
} from "./workflow";

describe("workflow contracts", () => {
  it("exposes the explicit approval and execution states", () => {
    expect(WORKFLOW_RUN_STATUSES).toContain("waiting_for_approval");
    expect(WORKFLOW_RUN_STATUSES).toContain("ready_for_execution");
    expect(WORKFLOW_STEP_STATUSES).toContain("waiting");
  });

  it("requires an immutable snapshot version and captured timestamp", () => {
    const snapshot = workflowContextSnapshotSchema.parse({
      snapshotVersion: 1,
      project: { projectId: "project-1" },
      skill: {
        skillId: "character-builder",
        skillVersion: "1.0.0",
        operationId: "character.create_face_lock",
      },
      input: { characterEntityId: "character-1" },
      prerequisiteReport: { passed: true, checks: [] },
      canon: [],
      assets: [],
      protectedTbds: [],
      resolvedContext: null,
      capturedAt: "2026-08-28T00:00:00.000Z",
    });

    expect(snapshot.snapshotVersion).toBe(1);
  });
});
