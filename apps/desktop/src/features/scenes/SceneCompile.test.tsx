import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SceneCompile } from "./SceneCompile";
import {
  getCompileReadiness,
  listCinemaCompilations,
  listShots,
  setShotVideo,
  type Shot,
} from "./api";
import { advanceWorkflowRun, createWorkflowRun, listSkillOperations } from "../workflows/api";
import { listAssets } from "../assets/api";
import {
  listGenerationResults,
  promoteGeneratedArtifact,
} from "../generation/api";
import { ProviderModelFields } from "../providers/ProviderModelFields";
import type {
  AssetSummary,
  GenerationResultSetDetail,
  SkillOperation,
  WorkflowRunDetail,
} from "@cinematic/domain";

vi.mock("./api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./api")>();
  return {
    ...actual,
    getCompileReadiness: vi.fn(),
    listCinemaCompilations: vi.fn(),
    listShots: vi.fn(),
    setShotVideo: vi.fn(),
  };
});
vi.mock("../workflows/api", () => ({
  advanceWorkflowRun: vi.fn(),
  createWorkflowRun: vi.fn(),
  listSkillOperations: vi.fn(),
  resolveSkillRef: vi.fn(),
  getWorkflowRun: vi.fn(),
  listWorkflowRuns: vi.fn(),
  listWorkflowCharacters: vi.fn(),
  approveWorkflowStep: vi.fn(),
  rejectWorkflowStep: vi.fn(),
  cancelWorkflowRun: vi.fn(),
  cancelWorkflowExecution: vi.fn(),
  retryWorkflowExecution: vi.fn(),
  suggestVisualSpec: vi.fn(),
  saveProviderCredential: vi.fn(),
  removeProviderCredentials: vi.fn(),
  configureProvider: vi.fn(),
  validateProviderConfiguration: vi.fn(),
}));
vi.mock("../generation/api", () => ({
  listGenerationResults: vi.fn(),
  promoteGeneratedArtifact: vi.fn(),
  getGeneratedArtifact: vi.fn(),
}));
vi.mock("../assets/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../assets/api")>();
  return { ...actual, listAssets: vi.fn(), getAssetWithVersions: vi.fn() };
});
vi.mock("../providers/ProviderModelFields", async () => {
  const { useEffect } = await import("react");
  return {
    ProviderModelFields: ({ onChange }: { onChange(value: { providerId: string; modelId: string }): void }) => {
      // Simulates a connected video service being auto-selected once.
      useEffect(() => {
        onChange({ providerId: "fake_async_video", modelId: "fake-video-v1" });
      }, []);
      return null;
    },
  };
});
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `mock-asset://${path}`,
}));
vi.mock("../workflows/WorkflowRunView", () => ({
  WorkflowRunView: () => <div data-testid="run-view" />,
}));

const shot: Shot = {
  id: "shot-1",
  sceneId: "scene-001",
  ordering: 0,
  durationSeconds: 4,
  keyframeAssetVersionId: "keyframe-v1",
  generatedVideoAssetVersionId: null,
  intent: "Establish",
  action: null,
  camera: null,
  createdAt: "now",
  updatedAt: "now",
};

const videoAsset: AssetSummary = {
  id: "video-asset-1",
  projectId: "project-1",
  type: "video",
  label: "Scene 001 — Video",
  ownerEntityId: "scene-001",
  canonicalVersionId: null,
  createdAt: "now",
  updatedAt: "now",
  versionCount: 0,
  canonicalVersionNumber: null,
  previewThumbnailPath: null,
};

const operation: SkillOperation = {
  id: "scene.generate_video",
  name: "Generate Scene Video",
  intentExamples: [],
  inputSchemaId: "generate_scene_video",
  prerequisites: [],
  tbdGuards: [],
  workflow: [],
  expectedOutput: {
    assetType: "video",
    mediaType: "video",
    desiredStatus: "candidate",
    ownerEntityInputRef: "sceneId",
  },
} as unknown as SkillOperation;

function completedVideoRun(): WorkflowRunDetail {
  return {
    run: {
      id: "video-run-1",
      projectId: "project-1",
      skillId: "scene-builder",
      skillVersion: "1.0.0",
      operationId: "scene.generate_video",
      status: "completed",
      inputJson: JSON.stringify({ sceneId: "scene-001" }),
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
    providerExecutions: [],
  } as unknown as WorkflowRunDetail;
}

const resultSets: GenerationResultSetDetail[] = [
  {
    resultSet: {
      id: "rs-1",
      projectId: "project-1",
      workflowRunId: "video-run-1",
      workflowStepKey: "execute",
      providerAttemptId: "attempt-1",
      mediaKind: "video",
      requestedOutputCount: 1,
      createdAt: "now",
    },
    artifacts: [
      {
        artifact: {
          id: "video-artifact-1",
          resultSetId: "rs-1",
          ordinal: 1,
          mediaKind: "video",
          mimeType: "video/mp4",
          width: null,
          height: null,
          byteSize: 24,
          sha256: "a".repeat(64),
          storagePath: "generated/video-run-1/attempt-1/0001.mp4",
          captureStatus: "available",
          createdAt: "now",
        },
        lineage: null,
      },
    ],
  },
];

describe("SceneCompile video generation flow", () => {
  beforeEach(() => {
    vi.mocked(getCompileReadiness).mockResolvedValue({
      sceneId: "scene-001",
      ready: true,
      blockers: [],
    } as never);
    vi.mocked(listCinemaCompilations).mockResolvedValue([
      {
        id: "comp-1",
        projectId: "project-1",
        sceneId: "scene-001",
        inputJson: "{}",
        compilationJson: "{}",
        exportPath: "prompts/cinema/comp-1.json",
        exportSha256: "b".repeat(64),
        createdAt: "now",
      },
    ] as never);
    vi.mocked(listShots).mockResolvedValue([{ ...shot }]);
    vi.mocked(listSkillOperations).mockResolvedValue([operation]);
    vi.mocked(listAssets).mockResolvedValue([videoAsset]);
    vi.mocked(listGenerationResults).mockResolvedValue(resultSets);
    vi.mocked(createWorkflowRun).mockResolvedValue(completedVideoRun());
    vi.mocked(advanceWorkflowRun).mockResolvedValue(completedVideoRun());
    vi.mocked(promoteGeneratedArtifact).mockResolvedValue({
      id: "video-version-1",
      assetId: "video-asset-1",
      versionNumber: 1,
      status: "canonical",
      filePath: "assets/video-asset-1/v001/video-version-1.mp4",
      thumbnailPath: "",
      sha256: "a".repeat(64),
      originalFilename: "0001.mp4",
      mimeType: "video/mp4",
      byteSize: 24,
      width: null,
      height: null,
      parentVersionId: null,
      createdAt: "now",
      origin: "generated",
      generationArtifactId: "video-artifact-1",
    } as never);
    vi.mocked(setShotVideo).mockResolvedValue(undefined);
  });

  it("renders the video candidate with a video element, saves it, and pins the exact version to the shot", async () => {
    const user = userEvent.setup();
    const projectRoot = "C:/proj";
    render(
      <SceneCompile projectRootPath={projectRoot} sceneId="scene-001" />,
    );

    // Start generation: the run goes straight to completed in this fixture.
    const generate = await screen.findByRole("button", {
      name: "Generate video from latest compilation",
    });
    await user.click(generate);

    // The candidate gallery appears with a <video> preview (no autoplay).
    // jsdom does not expose an implicit video role, so query the element.
    const video = document.querySelector("video");
    expect(video).not.toBeNull();
    expect(video).toHaveAttribute("preload", "metadata");
    expect(video).not.toHaveAttribute("autoplay");
    expect(screen.getByText("Video")).toBeInTheDocument();

    // Save the video into the scene's video asset.
    await user.click(await screen.findByRole("button", { name: "Save Video to Assets" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(
      within(dialog).getByRole("button", { name: "Save version" }),
    );
    await waitFor(() =>
      expect(promoteGeneratedArtifact).toHaveBeenCalledWith(
        "C:/proj",
        "video-artifact-1",
        "video-asset-1",
        false,
      ),
    );

    // The pin row appears for the unpinned shot; pinning calls set_shot_video
    // with the exact promoted version id.
    const pinButton = await screen.findByRole("button", { name: "Shot 1" });
    await user.click(pinButton);
    await waitFor(() =>
      expect(setShotVideo).toHaveBeenCalledWith(
        "C:/proj",
        "shot-1",
        "video-version-1",
      ),
    );
  });

  it("hides the generate section until a compilation exists", async () => {
    vi.mocked(listCinemaCompilations).mockResolvedValue([]);
    render(<SceneCompile projectRootPath="C:/proj" sceneId="scene-001" />);
    expect(
      screen.queryByRole("button", {
        name: "Generate video from latest compilation",
      }),
    ).toBeNull();
  });
});
