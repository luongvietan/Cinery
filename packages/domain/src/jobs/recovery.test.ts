import { describe, it, expect } from "vitest";
import {
  RecoveryDisposition,
  RecoveryClassification,
  classifyJobRecovery,
} from "./recovery";
import type { WorkflowRunRecord } from "../workflow";
import type { ProviderExecutionSummary } from "../workflow";
import type { AssetVersion, AssetVersionStatus } from "../asset";

describe("RecoveryDisposition", () => {
  it("defines all required disposition states", () => {
    const dispositions: RecoveryDisposition[] = [
      "nothing_required",
      "resume_local",
      "await_user_retry",
      "inspect_remote_result",
      "manual_resolution_required",
    ];
    dispositions.forEach((d) => expect(d).toBeDefined());
  });
});

describe("RecoveryClassification", () => {
  describe("Approval restart scenario", () => {
    it("classifies workflow waiting for approval as nothing_required with clear recovery state", () => {
      const run: WorkflowRunRecord = {
        id: "run-123",
        projectId: "proj-1",
        skillId: "face-lock",
        skillVersion: "1.0.0",
        operationId: "op-1",
        status: "waiting_for_approval",
        inputJson: "{}",
        prerequisiteReportJson: null,
        contextSnapshotJson: '{"snapshotVersion":1}',
        currentStepIndex: 1,
        failureCode: null,
        failureMessage: null,
        createdAt: "2024-01-01T00:00:00Z",
        updatedAt: "2024-01-01T00:00:00Z",
        completedAt: null,
      };

      const classification = classifyJobRecovery({ type: "workflow", run });

      expect(classification.jobType).toBe("workflow");
      expect(classification.jobId).toBe("run-123");
      expect(classification.disposition).toBe("nothing_required");
      expect(classification.explanation).toContain("awaiting");
      expect(classification.explanation).toContain("approval");
      expect(classification.userAction).toBeNull();
    });
  });

  describe("Provider failure scenario", () => {
    it("classifies failed provider execution as await_user_retry with no phantom asset", () => {
      const execution: ProviderExecutionSummary = {
        id: "exec-456",
        stepDefinitionId: "step-1",
        attemptNumber: 1,
        providerId: "openai",
        modelId: "dall-e-3",
        adapterVersion: 1,
        status: "failed",
        providerJobId: "remote-job-123",
        normalizedErrorJson: '{"code":"rate_limit_exceeded"}',
        startedAt: "2024-01-01T00:00:00Z",
        completedAt: "2024-01-01T00:05:00Z",
      };

      const classification = classifyJobRecovery({
        type: "provider",
        execution,
        assetVersionCreated: false,
      });

      expect(classification.jobType).toBe("provider");
      expect(classification.jobId).toBe("exec-456");
      expect(classification.disposition).toBe("await_user_retry");
      expect(classification.explanation).toContain("Provider failed");
      expect(classification.explanation).toContain("no output asset");
      expect(classification.userAction).toBe("explicit_retry");
    });

    it("detects phantom asset creation as manual_resolution_required", () => {
      const execution: ProviderExecutionSummary = {
        id: "exec-789",
        stepDefinitionId: "step-1",
        attemptNumber: 1,
        providerId: "openai",
        modelId: "dall-e-3",
        adapterVersion: 1,
        status: "failed",
        providerJobId: "remote-job-456",
        normalizedErrorJson: '{"code":"server_error"}',
        startedAt: "2024-01-01T00:00:00Z",
        completedAt: "2024-01-01T00:05:00Z",
      };

      const classification = classifyJobRecovery({
        type: "provider",
        execution,
        assetVersionCreated: true, // phantom!
      });

      expect(classification.disposition).toBe("manual_resolution_required");
      expect(classification.explanation).toContain("phantom");
    });
  });

  describe("QA failure scenario", () => {
    it("classifies failed QA as nothing_required with preserved failure details", () => {
      const assetVersion: AssetVersion = {
        id: "v-123",
        assetId: "asset-1",
        versionNumber: 3,
        status: "qa_failed",
        filePath: "path/to/file.png",
        thumbnailPath: "path/to/thumb.png",
        sha256: "abc123",
        originalFilename: "generated.png",
        mimeType: "image/png",
        byteSize: 1024,
        width: 512,
        height: 512,
        parentVersionId: null,
        createdAt: "2024-01-01T00:00:00Z",
        origin: "generated",
      };

      const qaFailureDetails = {
        checks: [
          { id: "check-1", type: "identity_similarity", status: "fail", label: "Face match" },
          { id: "check-2", type: "hair_consistency", status: "pass", label: "Hair" },
        ],
      };

      const classification = classifyJobRecovery({
        type: "qa",
        assetVersion,
        qaFailureDetails,
      });

      expect(classification.jobType).toBe("qa");
      expect(classification.jobId).toBe("v-123");
      expect(classification.disposition).toBe("nothing_required");
      expect(classification.explanation).toContain("QA failed");
      expect(classification.preservedFailureInfo).toBeDefined();
      expect(classification.preservedFailureInfo?.checks).toEqual(qaFailureDetails.checks);
      expect(classification.userAction).toBe("inspect_and_repair");
    });
  });

  describe("Repair scenario", () => {
    it("maintains parent relationship after restart", () => {
      const parentVersion: AssetVersion = {
        id: "v-parent",
        assetId: "asset-1",
        versionNumber: 3,
        status: "qa_failed",
        filePath: "path/to/parent.png",
        thumbnailPath: "path/to/parent-thumb.png",
        sha256: "parent-hash",
        originalFilename: "parent.png",
        mimeType: "image/png",
        byteSize: 1024,
        width: 512,
        height: 512,
        parentVersionId: null,
        createdAt: "2024-01-01T00:00:00Z",
        origin: "generated",
      };

      const childVersion: AssetVersion = {
        id: "v-child",
        assetId: "asset-1",
        versionNumber: 4,
        status: "repairing",
        filePath: "path/to/child.png",
        thumbnailPath: "path/to/child-thumb.png",
        sha256: "child-hash",
        originalFilename: "child.png",
        mimeType: "image/png",
        byteSize: 1024,
        width: 512,
        height: 512,
        parentVersionId: "v-parent",
        createdAt: "2024-01-01T00:01:00Z",
        origin: "generated",
      };

      const classification = classifyJobRecovery({
        type: "repair",
        parentVersion,
        childVersion,
      });

      expect(classification.jobType).toBe("repair");
      expect(classification.jobId).toBe("v-child");
      expect(classification.disposition).toBe("resume_local");
      expect(classification.parentVersionId).toBe("v-parent");
      expect(classification.explanation).toContain("repair");
      expect(classification.userAction).toBe("complete_repair");
    });
  });

  describe("Cancellation scenario", () => {
    it("keeps cancelled workflow cancelled and unreplayable", () => {
      const run: WorkflowRunRecord = {
        id: "run-cancelled",
        projectId: "proj-1",
        skillId: "face-lock",
        skillVersion: "1.0.0",
        operationId: "op-1",
        status: "cancelled",
        inputJson: "{}",
        prerequisiteReportJson: null,
        contextSnapshotJson: '{"snapshotVersion":1}',
        currentStepIndex: 0,
        failureCode: null,
        failureMessage: null,
        createdAt: "2024-01-01T00:00:00Z",
        updatedAt: "2024-01-01T00:01:00Z",
        completedAt: "2024-01-01T00:01:00Z",
      };

      const classification = classifyJobRecovery({ type: "workflow", run });

      expect(classification.jobType).toBe("workflow");
      expect(classification.jobId).toBe("run-cancelled");
      expect(classification.disposition).toBe("nothing_required");
      expect(classification.explanation).toContain("cancelled");
      expect(classification.userAction).toBeNull();
    });
  });

  describe("Cinema compile scenario", () => {
    it("keeps cinema compile result deterministic and inspectable", () => {
      const compilation = {
        id: "compile-123",
        cinemaId: "cinema-1",
        status: "completed" as const,
        resultJson: JSON.stringify({
          templateVersion: "1.0.0",
          timestampMs: 1704067200000,
          outputPath: "path/to/export.md",
        }),
        createdAt: "2024-01-01T00:00:00Z",
      };

      const classification = classifyJobRecovery({
        type: "cinema_compile",
        compilation,
      });

      expect(classification.jobType).toBe("cinema_compile");
      expect(classification.jobId).toBe("compile-123");
      expect(classification.disposition).toBe("nothing_required");
      expect(classification.explanation).toContain("Cinema");
      expect(classification.explanation).toContain("deterministic");
      expect(classification.userAction).toBeNull();
    });
  });
});

describe("Recovery classification integration", () => {
  it("handles mixed job types in a project recovery scan", () => {
    const jobs = [
      // Approval waiting
      {
        type: "workflow" as const,
        run: {
          id: "run-1",
          projectId: "proj-1",
          skillId: "face-lock",
          skillVersion: "1.0.0",
          operationId: "op-1",
          status: "waiting_for_approval" as const,
          inputJson: "{}",
          prerequisiteReportJson: null,
          contextSnapshotJson: '{"snapshotVersion":1}',
          currentStepIndex: 1,
          failureCode: null,
          failureMessage: null,
          createdAt: "2024-01-01T00:00:00Z",
          updatedAt: "2024-01-01T00:00:00Z",
          completedAt: null,
        },
      },
      // Provider failure
      {
        type: "provider" as const,
        execution: {
          id: "exec-1",
          stepDefinitionId: "step-1",
          attemptNumber: 1,
          providerId: "openai",
          modelId: "dall-e-3",
          adapterVersion: 1,
          status: "failed" as const,
          providerJobId: "remote-123",
          normalizedErrorJson: "{}",
          startedAt: "2024-01-01T00:00:00Z",
          completedAt: "2024-01-01T00:05:00Z",
        },
        assetVersionCreated: false,
      },
    ];

    const classifications = jobs.map(classifyJobRecovery);

    expect(classifications).toHaveLength(2);
    expect(classifications[0].disposition).toBe("nothing_required");
    expect(classifications[1].disposition).toBe("await_user_retry");
  });
});
