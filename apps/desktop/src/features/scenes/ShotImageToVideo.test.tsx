import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi, beforeEach } from "vitest";
import type { ShotVideoPromotionResult, WorkflowRunDetail } from "@cinematic/domain";
import { ShotImageToVideo } from "./ShotImageToVideo";
import type { Shot } from "./api";
import { advanceWorkflowRun, createWorkflowRun, getProviderCapabilities, getProviderConfigurationStatus, getWorkflowRun, listCustomProviders, listProviderModels, listProviders, listWorkflowRuns } from "../workflows/api";
import { getShotImageToVideoSource, promoteShotVideoCandidate } from "./api";
import { listGenerationResults } from "../generation/api";

vi.mock("../workflows/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `asset://localhost/${path}`,
}));
vi.mock("./api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./api")>();
  return {
    ...actual,
    getShotImageToVideoSource: vi.fn(),
    promoteShotVideoCandidate: vi.fn(),
  };
});
vi.mock("../generation/api");

const keyframeSource = {
  assetId: "asset-kf",
  assetVersionId: "kf-v1",
  versionNumber: 1,
  filePath: "assets/kf-v1.png",
  thumbnailPath: "thumbnails/kf-v1.webp",
  mimeType: "image/png",
};

function shot(overrides: Partial<Shot> = {}): Shot {
  return {
    id: "shot-1",
    sceneId: "scene-1",
    ordering: 0,
    durationSeconds: 4,
    keyframeAssetVersionId: "kf-v1",
    generatedVideoAssetVersionId: null,
    intent: "Establish",
    action: null,
    camera: null,
    createdAt: "now",
    updatedAt: "now",
    ...overrides,
  };
}

function completedRun(): WorkflowRunDetail {
  return {
    run: {
      id: "run-1",
      projectId: "project-1",
      skillId: "scene-builder",
      skillVersion: "1.0.0",
      operationId: "shot.image_to_video",
      status: "completed",
      inputJson: JSON.stringify({ sceneId: "scene-1", shotId: "shot-1" }),
      prerequisiteReportJson: null,
      contextSnapshotJson: null,
      currentStepIndex: 6,
      failureCode: null,
      failureMessage: null,
      createdAt: "now",
      updatedAt: "now",
      completedAt: "now",
    },
    steps: [],
    events: [],
  };
}

function runningDetail(id: string): WorkflowRunDetail {
  return {
    run: {
      id,
      projectId: "project-1",
      skillId: "scene-builder",
      skillVersion: "1.0.0",
      operationId: "shot.image_to_video",
      status: "running",
      inputJson: JSON.stringify({ sceneId: "scene-1", shotId: "shot-1" }),
      prerequisiteReportJson: null,
      contextSnapshotJson: null,
      currentStepIndex: 4,
      failureCode: null,
      failureMessage: null,
      createdAt: "now",
      updatedAt: "now",
      completedAt: null,
    },
    steps: [
      {
        id: "step-1",
        workflowRunId: id,
        stepDefinitionId: "execute",
        stepIndex: 4,
        stepType: "execute",
        status: "running",
        inputJson: null,
        outputJson: null,
        startedAt: "now",
        completedAt: null,
      },
    ],
    events: [],
  };
}

describe("ShotImageToVideo", () => {
  beforeEach(() => {
    vi.mocked(getShotImageToVideoSource).mockResolvedValue(keyframeSource);
    vi.mocked(createWorkflowRun).mockResolvedValue(completedRun());
    vi.mocked(advanceWorkflowRun).mockResolvedValue(completedRun());
    vi.mocked(listWorkflowRuns).mockResolvedValue([]);
    vi.mocked(getWorkflowRun).mockResolvedValue(completedRun());
    vi.mocked(listGenerationResults).mockResolvedValue([]);
    vi.mocked(listProviders).mockResolvedValue(["i2v"]);
    vi.mocked(listCustomProviders).mockResolvedValue([]);
    vi.mocked(listProviderModels).mockResolvedValue(["motion-v1"]);
    vi.mocked(getProviderCapabilities).mockResolvedValue({
      mediaTypes: ["video"], supportsSeed: false, supportsNegativePrompt: false,
      supportsReferenceImage: false, supportsImageEdit: false, supportsMultipleReferenceImages: false,
      supportsImageToVideo: true, supportsCancel: false, supportsProgress: false,
      supportedAspectRatios: [], supportedModels: ["motion-v1"],
    });
    vi.mocked(getProviderConfigurationStatus).mockResolvedValue({
      providerId: "i2v", enabled: true, credentialConfigured: true,
      defaultModel: "motion-v1", models: ["motion-v1"],
    });
    vi.mocked(promoteShotVideoCandidate).mockImplementation(async (_root, _shotId, artifactId, expected) => {
      const result: ShotVideoPromotionResult = {
        shotId: "shot-1",
        artifactId,
        assetVersionId: `video-version-${artifactId}`,
        previousAssetVersionId: expected ?? null,
      };
      return result;
    });
  });

  it("disables generation without an exact keyframe", async () => {
    vi.mocked(getShotImageToVideoSource).mockRejectedValue({ code: "SOURCE_KEYFRAME_MISSING" });
    render(
      <ShotImageToVideo projectRootPath="C:/project" sceneId="scene-1" shot={shot({ keyframeAssetVersionId: null })} onShotChanged={vi.fn()} />,
    );
    expect(await screen.findByRole("button", { name: "Generate Video" })).toBeDisabled();
    expect(screen.getByText("Add or generate a keyframe first.")).toBeInTheDocument();
  });

  it("creates the exact Shot I2V payload once on rapid clicks", async () => {
    const user = userEvent.setup();
    render(
      <ShotImageToVideo projectRootPath="C:/project" sceneId="scene-1" shot={shot()} onShotChanged={vi.fn()} />,
    );
    await user.type(await screen.findByLabelText("Motion prompt"), "Slow push-in");
    const button = await screen.findByRole("button", { name: "Generate Video" });
    await waitFor(() => expect(button).toBeEnabled());
    await Promise.all([user.click(button), user.click(button)]);
    await waitFor(() => expect(createWorkflowRun).toHaveBeenCalledTimes(1));
    expect(createWorkflowRun).toHaveBeenCalledWith("C:/project", "scene-builder", "1.0.0", "shot.image_to_video", {
      sceneId: "scene-1",
      shotId: "shot-1",
      providerId: "i2v",
      modelId: "motion-v1",
      prompt: "Slow push-in",
      generationParameters: { durationSeconds: 4 },
    });
  });

  it("promotes the exact candidate with the current pin as the expected value", async () => {
    vi.mocked(listGenerationResults).mockResolvedValue([
      {
        resultSet: {
          id: "rs-1",
          projectId: "project-1",
          workflowRunId: "run-1",
          workflowStepKey: "execute",
          providerAttemptId: "attempt-1",
          mediaKind: "video",
          requestedOutputCount: 1,
          createdAt: "now",
        },
        artifacts: [
          {
            artifact: {
              id: "artifact-1",
              resultSetId: "rs-1",
              ordinal: 1,
              mediaKind: "video",
              mimeType: "video/mp4",
              width: null,
              height: null,
              byteSize: 24,
              sha256: "a".repeat(64),
              storagePath: "generations/run-1/a.mp4",
              captureStatus: "available",
              captureErrorCode: null,
              createdAt: "now",
            },
            lineage: {
              artifactId: "artifact-1",
              workflowRunId: "run-1",
              workflowStepKey: "execute",
              workflowDefinitionId: "scene-builder",
              workflowVersion: "1.0.0",
              skillId: "scene-builder",
              skillVersion: "1.0.0",
              compiledExecutionArtifactId: "c",
              compiledRequestSha256: "b".repeat(64),
              canonSnapshotId: null,
              canonSnapshotSha256: null,
              providerAttemptId: "attempt-1",
              providerId: "i2v",
              modelId: "motion-v1",
              sourceAssetVersionIds: ["kf-v1"],
              createdAt: "now",
            },
          },
        ],
      },
    ]);
    const onShotChanged = vi.fn();
    const user = userEvent.setup();
    render(
      <ShotImageToVideo projectRootPath="C:/project" sceneId="scene-1" shot={shot({ generatedVideoAssetVersionId: "current-pin" })} onShotChanged={onShotChanged} />,
    );
    await user.click(await screen.findByRole("button", { name: "Generate Video" }));
    const useForShot = await screen.findByRole("button", { name: "Use for Shot" });
    await user.click(useForShot);
    await waitFor(() => expect(promoteShotVideoCandidate).toHaveBeenCalledWith(
      "C:/project",
      "shot-1",
      "artifact-1",
      "current-pin",
    ));
    expect(onShotChanged).toHaveBeenCalled();
  });

  it("marks the current pin so the user sees which candidate is live", async () => {
    render(
      <ShotImageToVideo projectRootPath="C:/project" sceneId="scene-1" shot={shot({ generatedVideoAssetVersionId: "video-version-artifact-1" })} onShotChanged={vi.fn()} />,
    );
    expect(await screen.findByText(/current Shot video/)).toBeInTheDocument();
  });

  it("restores the latest persisted run for this Shot after remount", async () => {
    vi.mocked(listWorkflowRuns).mockResolvedValue([
      {
        id: "other",
        projectId: "project-1",
        skillId: "scene-builder",
        skillVersion: "1.0.0",
        operationId: "shot.image_to_video",
        status: "running",
        inputJson: JSON.stringify({ shotId: "shot-2" }),
        prerequisiteReportJson: null,
        contextSnapshotJson: null,
        currentStepIndex: 4,
        failureCode: null,
        failureMessage: null,
        createdAt: "t1",
        updatedAt: "t1",
        completedAt: null,
      },
      {
        id: "mine",
        projectId: "project-1",
        skillId: "scene-builder",
        skillVersion: "1.0.0",
        operationId: "shot.image_to_video",
        status: "running",
        inputJson: JSON.stringify({ shotId: "shot-1" }),
        prerequisiteReportJson: null,
        contextSnapshotJson: null,
        currentStepIndex: 4,
        failureCode: null,
        failureMessage: null,
        createdAt: "t2",
        updatedAt: "t2",
        completedAt: null,
      },
    ]);
    vi.mocked(getWorkflowRun).mockResolvedValue(runningDetail("mine"));
    render(
      <ShotImageToVideo projectRootPath="C:/project" sceneId="scene-1" shot={shot()} onShotChanged={vi.fn()} />,
    );
    expect(await screen.findByText("Generating…")).toBeInTheDocument();
    expect(getWorkflowRun).toHaveBeenCalledWith("C:/project", "mine");
  });

  it("keeps the last valid run across a transient read failure", async () => {
    vi.mocked(listWorkflowRuns).mockResolvedValue([
      {
        id: "mine",
        projectId: "project-1",
        skillId: "scene-builder",
        skillVersion: "1.0.0",
        operationId: "shot.image_to_video",
        status: "running",
        inputJson: JSON.stringify({ shotId: "shot-1" }),
        prerequisiteReportJson: null,
        contextSnapshotJson: null,
        currentStepIndex: 4,
        failureCode: null,
        failureMessage: null,
        createdAt: "t1",
        updatedAt: "t1",
        completedAt: null,
      },
    ]);
    vi.mocked(getWorkflowRun).mockResolvedValueOnce(runningDetail("mine")).mockRejectedValueOnce(new Error("temporary"));
    const { rerender } = render(
      <ShotImageToVideo projectRootPath="C:/project" sceneId="scene-1" shot={shot()} onShotChanged={vi.fn()} />,
    );
    expect(await screen.findByText("Generating…")).toBeInTheDocument();
    // Remount: restoration re-reads, the read fails, the panel must keep the
    // previous detail rather than crashing or clearing.
    rerender(
      <ShotImageToVideo projectRootPath="C:/project" sceneId="scene-1" shot={shot()} onShotChanged={vi.fn()} />,
    );
    await waitFor(() => expect(getWorkflowRun).toHaveBeenCalledTimes(2));
    expect(screen.getByText("Generating…")).toBeInTheDocument();
    expect(screen.queryByText("temporary")).not.toBeInTheDocument();
  });
});
