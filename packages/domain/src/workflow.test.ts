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

  it("validates nested snapshot records instead of accepting arbitrary values", () => {
    const base = {
      snapshotVersion: 1,
      project: { projectId: "project-1" },
      skill: {
        skillId: "character-builder",
        skillVersion: "1.0.0",
        operationId: "character.create_face_lock",
      },
      input: {},
      prerequisiteReport: { passed: true, checks: [] },
      canon: [],
      assets: [],
      protectedTbds: [],
      resolvedContext: null,
      capturedAt: "2026-08-28T00:00:00.000Z",
    };

    expect(() =>
      workflowContextSnapshotSchema.parse({
        ...base,
        canon: [
          {
            entityId: "character-1",
            entityType: "character",
            sectionId: "section-1",
            sectionKey: "role_tag",
            revision: 1,
            status: "draft",
            value: { text: "Lead" },
          },
        ],
      }),
    ).toThrow();

    expect(() =>
      workflowContextSnapshotSchema.parse({
        ...base,
        prerequisiteReport: {
          passed: true,
          checks: [
            {
              id: "check-1",
              prerequisite: {
                type: "canon_entity_exists",
                entityType: "character",
                inputRef: "characterEntityId",
                unexpected: true,
              },
              status: "pass",
              message: "ok",
              resolvedRef: "character-1",
            },
          ],
        },
      }),
    ).toThrow();

    expect(() =>
      workflowContextSnapshotSchema.parse({
        ...base,
        assets: [
          {
            assetId: "asset-1",
            assetVersionId: "version-1",
            assetType: "face_lock",
            versionNumber: 1,
            status: "candidate",
            path: "assets/face-lock.png",
          },
        ],
      }),
    ).toThrow();

    expect(() =>
      workflowContextSnapshotSchema.parse({
        ...base,
        protectedTbds: [
          {
            id: "tbd-1",
            projectId: "project-1",
            canonEntityId: null,
            sectionKey: null,
            topic: "Unknown",
            note: null,
            protected: true,
            status: "pending",
            resolutionText: null,
            createdAt: "2026-08-28T00:00:00.000Z",
            updatedAt: "2026-08-28T00:00:00.000Z",
            resolvedAt: null,
          },
        ],
      }),
    ).toThrow();
  });
});
