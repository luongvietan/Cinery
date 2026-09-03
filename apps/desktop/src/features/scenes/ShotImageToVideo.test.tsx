import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi, beforeEach } from "vitest";
import type { GenerationResultSetDetail, WorkflowRunDetail } from "@cinematic/domain";
import { ShotImageToVideo } from "./ShotImageToVideo";
import type { Shot } from "./api";
import { advanceWorkflowRun, createWorkflowRun, getProviderCapabilities, getProviderConfigurationStatus, getWorkflowRun, listCustomProviders, listProviderModels, listProviders, listWorkflowRuns } from "../workflows/api";
import { getShotImageToVideoSource, listShotVideoCandidates, promoteShotVideoCandidate } from "./api";
import { listGenerationResults } from "../generation/api";
import { getAssetWithVersions, listAssets } from "../assets/api";
import * as qaApi from "../qa/api";

vi.mock("../workflows/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `asset://localhost/${path}`,
}));
vi.mock("./api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./api")>();
  return {
    ...actual,
    getShotImageToVideoSource: vi.fn(),
    listShotVideoCandidates: vi.fn(),
    promoteShotVideoCandidate: vi.fn(),
  };
});
vi.mock("../generation/api");
vi.mock("../assets/api");
vi.mock("../qa/api");

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

function runWithStatus(
  status: WorkflowRunDetail["run"]["status"],
  id = "run-1",
): WorkflowRunDetail {
  return {
    run: {
      id,
      projectId: "project-1",
      skillId: "scene-builder",
      skillVersion: "1.0.0",
      operationId: "shot.image_to_video",
      status,
      inputJson: JSON.stringify({ sceneId: "scene-1", shotId: "shot-1" }),
      prerequisiteReportJson: null,
      contextSnapshotJson: null,
      currentStepIndex: 6,
      failureCode: null,
      failureMessage: null,
      createdAt: "now",
      updatedAt: "now",
      completedAt: status === "completed" || status === "cancelled" || status === "failed" ? "now" : null,
    },
    steps: [],
    events: [],
  };
}

function completedRun(): WorkflowRunDetail {
  return runWithStatus("completed");
}

function generationResults(): GenerationResultSetDetail[] {
  return [
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
  ];
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
    vi.clearAllMocks();
    vi.mocked(getShotImageToVideoSource).mockResolvedValue(keyframeSource);
    vi.mocked(listShotVideoCandidates).mockResolvedValue([]);
    vi.mocked(createWorkflowRun).mockResolvedValue(completedRun());
    vi.mocked(advanceWorkflowRun).mockResolvedValue(completedRun());
    vi.mocked(listWorkflowRuns).mockResolvedValue([]);
    vi.mocked(getWorkflowRun).mockResolvedValue(completedRun());
    vi.mocked(listGenerationResults).mockResolvedValue([]);
    vi.mocked(listAssets).mockResolvedValue([]);
    vi.mocked(getAssetWithVersions).mockRejectedValue(new Error("asset not found"));
    vi.mocked(qaApi.listQaRuns).mockResolvedValue([]);
    vi.mocked(listProviders).mockResolvedValue(["i2v"]);
    vi.mocked(listCustomProviders).mockResolvedValue([]);
    vi.mocked(listProviderModels).mockResolvedValue(["motion-v1"]);
    vi.mocked(getProviderCapabilities).mockResolvedValue({
      mediaTypes: ["video"], supportsSeed: false, supportsNegativePrompt: false,
      supportsReferenceImage: true, supportsImageEdit: false, supportsMultipleReferenceImages: false,
      supportsImageToVideo: true, supportsCancel: false, supportsProgress: false,
      supportedAspectRatios: [], supportedModels: ["motion-v1"],
    });
    vi.mocked(getProviderConfigurationStatus).mockResolvedValue({
      providerId: "i2v", enabled: true, credentialConfigured: true,
      defaultModel: "motion-v1", models: ["motion-v1"],
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

  it("does not allow generation with a blank motion prompt", async () => {
    render(
      <ShotImageToVideo projectRootPath="C:/project" sceneId="scene-1" shot={shot()} onShotChanged={vi.fn()} />,
    );

    expect(await screen.findByRole("button", { name: "Generate Video" })).toBeDisabled();
    expect(screen.getByText("Add a motion description first.")).toBeInTheDocument();
  });

  it("does not allow generation with a duration outside the supported range", async () => {
    const user = userEvent.setup();
    render(
      <ShotImageToVideo projectRootPath="C:/project" sceneId="scene-1" shot={shot()} onShotChanged={vi.fn()} />,
    );

    await user.type(await screen.findByLabelText("Motion prompt"), "Slow push-in");
    const duration = await screen.findByLabelText("Duration (s)");
    await user.clear(duration);
    await user.type(duration, "120");

    expect(screen.getByRole("button", { name: "Generate Video" })).toBeDisabled();
    expect(screen.getByText("Duration must be between 0.5 and 30 seconds.")).toBeInTheDocument();
  });

  it("creates the exact Shot I2V payload once on rapid clicks", async () => {
    let resolveCreation!: (detail: WorkflowRunDetail) => void;
    vi.mocked(createWorkflowRun).mockImplementationOnce(
      () => new Promise((resolve) => { resolveCreation = resolve; }),
    );
    const user = userEvent.setup();
    render(
      <ShotImageToVideo projectRootPath="C:/project" sceneId="scene-1" shot={shot()} onShotChanged={vi.fn()} />,
    );
    await user.type(await screen.findByLabelText("Motion prompt"), "Slow push-in");
    const button = await screen.findByRole("button", { name: "Generate Video" });
    await waitFor(() => expect(button).toBeEnabled());
    const clicks = Promise.all([user.click(button), user.click(button)]);
    await waitFor(() => expect(createWorkflowRun).toHaveBeenCalledTimes(1));
    resolveCreation(completedRun());
    await clicks;
    expect(createWorkflowRun).toHaveBeenCalledWith("C:/project", "scene-builder", "1.0.0", "shot.image_to_video", {
      sceneId: "scene-1",
      shotId: "shot-1",
      providerId: "i2v",
      modelId: "motion-v1",
      prompt: "Slow push-in",
      generationParameters: { durationSeconds: 4 },
    });
  });

  it("marks the current pin so the user sees which candidate is live", async () => {
    render(
      <ShotImageToVideo projectRootPath="C:/project" sceneId="scene-1" shot={shot({ generatedVideoAssetVersionId: "video-version-artifact-1" })} onShotChanged={vi.fn()} />,
    );
    expect(await screen.findByText(/current Shot video/)).toBeInTheDocument();
  });

  it("shows the review workspace candidates resolved by the backend read model", async () => {
    vi.mocked(listShotVideoCandidates).mockResolvedValue([
      {
        assetVersionId: "video-v1", versionNumber: 1, shotId: "shot-1", sceneId: "scene-1",
        createdAt: "2026-09-03T00:00:00Z", filePath: "assets/video-v1.mp4", mimeType: "video/mp4",
        byteSize: 24, reviewState: "active", isCanonical: false, qaOverallStatus: null,
        qaRunCount: 0, providerId: "i2v", modelId: "motion-v1", workflowRunId: "run-1",
        sourceAssetVersionId: "kf-v1", sourceKeyframeIsCurrent: true,
      },
    ]);
    render(
      <ShotImageToVideo projectRootPath="C:/project" sceneId="scene-1" shot={shot()} onShotChanged={vi.fn()} />,
    );
    expect(await screen.findByRole("button", { name: "V01" })).toBeInTheDocument();
    expect(listShotVideoCandidates).toHaveBeenCalledWith("C:/project", "shot-1");
  });

  it("delegates regeneration to the existing generation path without touching the pin", async () => {
    vi.mocked(listShotVideoCandidates).mockResolvedValue([
      {
        assetVersionId: "video-v1", versionNumber: 1, shotId: "shot-1", sceneId: "scene-1",
        createdAt: "2026-09-03T00:00:00Z", filePath: "assets/video-v1.mp4", mimeType: "video/mp4",
        byteSize: 24, reviewState: "active", isCanonical: true, qaOverallStatus: "pass",
        qaRunCount: 1, providerId: "i2v", modelId: "motion-v1", workflowRunId: "run-1",
        sourceAssetVersionId: "kf-v1", sourceKeyframeIsCurrent: true,
      },
    ]);
    const user = userEvent.setup();
    render(
      <ShotImageToVideo projectRootPath="C:/project" sceneId="scene-1" shot={shot()} onShotChanged={vi.fn()} />,
    );

    // The canonical candidate is visible; a new generation can be started
    // (Regenerate) which goes through the existing createWorkflowRun path.
    expect(await screen.findByTestId("canonical-badge-video-v1")).toBeInTheDocument();
    await user.type(await screen.findByLabelText("Motion prompt"), "Slow push-in");
    const button = await screen.findByRole("button", { name: "Generate Video" });
    await waitFor(() => expect(button).toBeEnabled());
    await user.click(button);
    await waitFor(() => expect(createWorkflowRun).toHaveBeenCalledTimes(1));
    expect(createWorkflowRun).toHaveBeenCalledWith(
      "C:/project", "scene-builder", "1.0.0", "shot.image_to_video",
      expect.objectContaining({ shotId: "shot-1" }),
    );
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

  it("restores persisted candidates for a completed run", async () => {
    const persisted = completedRun();
    vi.mocked(listWorkflowRuns).mockResolvedValue([persisted.run]);
    vi.mocked(getWorkflowRun).mockResolvedValue(persisted);
    vi.mocked(listGenerationResults).mockResolvedValue(generationResults());
    vi.mocked(listShotVideoCandidates).mockResolvedValue([
      {
        assetVersionId: "video-v1", versionNumber: 1, shotId: "shot-1", sceneId: "scene-1",
        createdAt: "2026-09-03T00:00:00Z", filePath: "assets/video-v1.mp4", mimeType: "video/mp4",
        byteSize: 24, reviewState: "active", isCanonical: false, qaOverallStatus: null,
        qaRunCount: 0, providerId: "i2v", modelId: "motion-v1", workflowRunId: "run-1",
        sourceAssetVersionId: "kf-v1", sourceKeyframeIsCurrent: true,
      },
    ]);

    render(
      <ShotImageToVideo projectRootPath="C:/project" sceneId="scene-1" shot={shot()} onShotChanged={vi.fn()} />,
    );

    expect(await screen.findByRole("button", { name: "V01" })).toBeInTheDocument();
    expect(listGenerationResults).toHaveBeenCalledWith("C:/project", "run-1");
  });

  it("never auto-runs Video QA or auto-promotes a restored candidate", async () => {
    const persisted = completedRun();
    vi.mocked(listWorkflowRuns).mockResolvedValue([persisted.run]);
    vi.mocked(getWorkflowRun).mockResolvedValue(persisted);
    vi.mocked(listGenerationResults).mockResolvedValue(generationResults());
    vi.mocked(listShotVideoCandidates).mockResolvedValue([
      {
        assetVersionId: "video-v1", versionNumber: 1, shotId: "shot-1", sceneId: "scene-1",
        createdAt: "2026-09-03T00:00:00Z", filePath: "assets/video-v1.mp4", mimeType: "video/mp4",
        byteSize: 24, reviewState: "active", isCanonical: false, qaOverallStatus: "fail",
        qaRunCount: 1, providerId: "i2v", modelId: "motion-v1", workflowRunId: "run-1",
        sourceAssetVersionId: "kf-v1", sourceKeyframeIsCurrent: true,
      },
    ]);

    render(
      <ShotImageToVideo projectRootPath="C:/project" sceneId="scene-1" shot={shot()} onShotChanged={vi.fn()} />,
    );

    expect(await screen.findByRole("button", { name: "V01" })).toBeInTheDocument();
    expect(qaApi.createVideoQaWorkflow).not.toHaveBeenCalled();
    expect(promoteShotVideoCandidate).not.toHaveBeenCalled();
    expect(createWorkflowRun).not.toHaveBeenCalled();
  });

  it.each(["completed", "cancelled", "failed", "rejected"] as const)(
    "allows a new generation after a %s run",
    async (status) => {
      const persisted = runWithStatus(status);
      vi.mocked(listWorkflowRuns).mockResolvedValue([persisted.run]);
      vi.mocked(getWorkflowRun).mockResolvedValue(persisted);
      const user = userEvent.setup();

      render(
        <ShotImageToVideo projectRootPath="C:/project" sceneId="scene-1" shot={shot()} onShotChanged={vi.fn()} />,
      );
      await waitFor(() => expect(getWorkflowRun).toHaveBeenCalledWith("C:/project", "run-1"));
      await user.type(await screen.findByLabelText("Motion prompt"), "Slow push-in");
      const button = await screen.findByRole("button", { name: "Generate Video" });
      await waitFor(() => expect(button).toBeEnabled());
      await user.click(button);

      await waitFor(() => expect(createWorkflowRun).toHaveBeenCalledTimes(1));
    },
  );

  it("requires providers to accept the source reference image", async () => {
    vi.mocked(getProviderCapabilities).mockResolvedValue({
      mediaTypes: ["video"], supportsSeed: false, supportsNegativePrompt: false,
      supportsReferenceImage: false, supportsImageEdit: false, supportsMultipleReferenceImages: false,
      supportsImageToVideo: true, supportsCancel: false, supportsProgress: false,
      supportedAspectRatios: [], supportedModels: ["motion-v1"],
    });

    render(
      <ShotImageToVideo projectRootPath="C:/project" sceneId="scene-1" shot={shot()} onShotChanged={vi.fn()} />,
    );

    const option = await screen.findByRole("option", { name: /i2v/ });
    expect(option).toBeDisabled();
    expect(option).toHaveTextContent("cannot accept reference images");
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
    // A project-context refresh re-reads, the read fails, and the panel must
    // keep the previous detail rather than crashing or clearing.
    rerender(
      <ShotImageToVideo projectRootPath="C:/project-refreshed" sceneId="scene-1" shot={shot()} onShotChanged={vi.fn()} />,
    );
    await waitFor(() => expect(getWorkflowRun).toHaveBeenCalledTimes(2));
    expect(screen.getByText("Generating…")).toBeInTheDocument();
    expect(screen.queryByText("temporary")).not.toBeInTheDocument();
  });
});
