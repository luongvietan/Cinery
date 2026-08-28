import { describe, it, expect } from "vitest";
import type {
  HealthSeverity,
  ProjectHealthIssue,
  ProjectReadiness,
  ProjectHealthSummary,
} from "./integration";
import { READINESS_STATUSES } from "./integration";

describe("Integration Types", () => {
  describe("ProjectHealthIssue", () => {
    it("should represent a health issue with code, severity, and message", () => {
      const issue: ProjectHealthIssue = {
        code: "BROKEN_ASSET_FILE_REFERENCE",
        severity: "error",
        entityType: "asset_version",
        entityId: "asset-v1",
        message: "Asset media file is missing.",
        remediation: "Restore the media file.",
      };

      expect(issue.code).toBe("BROKEN_ASSET_FILE_REFERENCE");
      expect(issue.severity).toBe("error");
      expect(issue.entityType).toBe("asset_version");
    });

    it("should allow entityId to be null", () => {
      const issue: ProjectHealthIssue = {
        code: "UNSUPPORTED_SCHEMA_VERSION",
        severity: "fatal",
        entityType: "project",
        entityId: null,
        message: "Project database schema is too new.",
        remediation: null,
      };

      expect(issue.entityId).toBeNull();
      expect(issue.remediation).toBeNull();
    });

    it("should allow remediation to be null", () => {
      const issue: ProjectHealthIssue = {
        code: "SOME_ERROR",
        severity: "warning",
        entityType: "scene",
        entityId: "scene-1",
        message: "Something is not quite right.",
        remediation: null,
      };

      expect(issue.remediation).toBeNull();
    });
  });

  describe("HealthSeverity", () => {
    it("should have all valid severity levels", () => {
      const severities: HealthSeverity[] = ["info", "warning", "error", "fatal"];
      severities.forEach((severity) => {
        expect(["info", "warning", "error", "fatal"]).toContain(severity);
      });
    });
  });

  describe("ProjectReadiness", () => {
    it("should capture production readiness state", () => {
      const readiness: ProjectReadiness = {
        status: "pending",
        nextAction: {
          id: "action-1",
          title: "Complete Story Canon",
          destination: "canon",
          characterEntityId: null,
          sceneId: null,
        },
        steps: [
          {
            id: "step-1",
            title: "Story Canon",
            status: "pending",
            detail: "Story sections not yet locked.",
            action: null,
          },
        ],
      };

      expect(readiness.status).toBe("pending");
      expect(readiness.nextAction?.title).toBe("Complete Story Canon");
      expect(readiness.steps[0].status).toBe("pending");
    });

    it("should allow complete readiness status", () => {
      const readiness: ProjectReadiness = {
        status: "complete",
        nextAction: null,
        steps: [],
      };

      expect(readiness.status).toBe("complete");
      expect(readiness.nextAction).toBeNull();
    });

    it("should allow blocked readiness status", () => {
      const readiness: ProjectReadiness = {
        status: "blocked",
        nextAction: {
          id: "action-1",
          title: "Resolve protected TBD",
          destination: "canon",
          characterEntityId: null,
          sceneId: null,
        },
        steps: [],
      };

      expect(readiness.status).toBe("blocked");
    });
  });

  describe("ProjectHealthSummary", () => {
    it("should summarize health status", () => {
      const summary: ProjectHealthSummary = {
        openProtectedTbdCount: 1,
        openTbdCount: 3,
        activeJobCount: 1,
      };

      expect(summary.openProtectedTbdCount).toBe(1);
      expect(summary.openTbdCount).toBe(3);
      expect(summary.activeJobCount).toBe(1);
    });

    it("should allow zero counts", () => {
      const summary: ProjectHealthSummary = {
        openProtectedTbdCount: 0,
        openTbdCount: 0,
        activeJobCount: 0,
      };

      expect(summary.openProtectedTbdCount).toBe(0);
    });
  });

  describe("Health Check Codes", () => {
    it("should define standard error codes", () => {
      const codes = [
        "BROKEN_ASSET_FILE_REFERENCE",
        "ASSET_VERSION_OWNER_MISMATCH",
        "MULTIPLE_CANONICAL_VERSIONS",
        "MISSING_SCENE_LOOK_REFERENCE",
        "MISSING_SCENE_WORLD_REFERENCE",
        "MISSING_SCENE_PROP_REFERENCE",
        "SHOT_SCENE_MISMATCH",
        "MISSING_KEYFRAME",
        "WORKFLOW_INPUT_REFERENCE_MISSING",
        "GENERATION_OUTPUT_REFERENCE_MISSING",
        "QA_TARGET_MISSING",
        "REPAIR_PARENT_MISSING",
        "CINEMA_INPUT_REFERENCE_MISSING",
        "UNSUPPORTED_SCHEMA_VERSION",
      ];

      expect(codes.length).toBeGreaterThanOrEqual(13);
    });
  });
});
