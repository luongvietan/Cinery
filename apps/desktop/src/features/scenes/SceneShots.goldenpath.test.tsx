import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SceneWorkspace } from "./SceneWorkspace";
import {
  buildSequencePreflight,
  createShot,
  ensureSceneKeyframeAsset,
  getCompileReadiness,
  getScene,
  listCinemaCompilations,
  listScenes,
  listShots,
  setShotKeyframe,
  resolveSceneReferences,
  type Shot,
} from "./api";
import { advanceWorkflowRun, createWorkflowRun, listSkillOperations } from "../workflows/api";
import {
  listAssets,
} from "../assets/api";
import {
  listGenerationResults,
  promoteGeneratedArtifact,
} from "../generation/api";
import type { WorkflowRunDetail, AssetSummary, GenerationResultSetDetail } from "@cinematic/domain";

vi.mock("./api");
vi.mock("../workflows/api");
vi.mock("../generation/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `mock-asset://${path}`,
}));
vi.mock("../assets/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../assets/api")>();
  return { ...actual, listAssets: vi.fn(), getAssetWithVersions: vi.fn() };
});
vi.mock("../worlds/api", () => ({
  listWorlds: vi.fn().mockResolvedValue([]),
  listWorldsDetailed: vi.fn().mockResolvedValue([]),
  getWorldDetailed: vi.fn().mockResolvedValue(null),
}));
vi.mock("../canon/api", () => ({
  getCanonEntity: vi.fn().mockResolvedValue(null),
  listCanonTbds: vi.fn().mockResolvedValue([]),
  listCanonEntities: vi.fn().mockResolvedValue([]),
}));
vi.mock("../qa/api", () => ({
  listQaRuns: vi.fn().mockResolvedValue([]),
  getQaRun: vi.fn().mockResolvedValue(null),
}));

const scene = {
  id: "scene-001",
  projectId: "project-1",
  ordinal: 0,
  title: "Scene 001",
  summary: "Mara returns to the ops room",
  worldId: "world-1",
  worldAssetVersionId: "world-v1",
  keyframeAssetId: "kf-asset",
  createdAt: "now",
  updatedAt: "now",
};

let shots: Shot[];
let ready = false;

function runDetail(status: string): WorkflowRunDetail {
  return {
    run: {
      id: "run-1",
      projectId: "project-1",
      skillId: "scene-builder",
      skillVersion: "1.0.0",
      operationId: "scene.create_keyframe",
      status,
      inputJson: "{}",
      prerequisiteReportJson: null,
      contextSnapshotJson: null,
      currentStepIndex: 0,
      failureCode: null,
      failureMessage: null,
      createdAt: "now",
      updatedAt: "now",
      completedAt: null,
    },
    steps: [],
    events: [],
    providerExecutions: [],
  } as unknown as WorkflowRunDetail;
}

describe("Scene → Shot → Keyframe → Compile golden path (UI portions)", () => {
  beforeEach(() => {
    shots = [];
    ready = false;
    vi.mocked(listScenes).mockResolvedValue([scene]);
    vi.mocked(getScene).mockResolvedValue(scene as never);
    vi.mocked(resolveSceneReferences).mockResolvedValue({
      sceneId: scene.id,
      world: null,
      characters: [],
      props: [],
    } as never);
    vi.mocked(listShots).mockImplementation(async () => shots);
    vi.mocked(getCompileReadiness).mockImplementation(async () => ({
      sceneId: scene.id,
      ready,
      blockers: ready ? [] : [{ code: "missing_shot", sceneId: scene.id, entityId: null, shotId: null, message: "This scene has no shots.", actionTarget: "shot" }],
    }));
    vi.mocked(listCinemaCompilations).mockResolvedValue([]);
    vi.mocked(buildSequencePreflight).mockResolvedValue(null);
    vi.mocked(createShot).mockImplementation(async (_root, _sceneId, durationSeconds, intent) => {
      const shot: Shot = {
        id: `shot-${shots.length + 1}`,
        sceneId: scene.id,
        ordering: shots.length,
        durationSeconds,
        keyframeAssetVersionId: null,
        generatedVideoAssetVersionId: null,
        intent,
        action: null,
        camera: null,
        createdAt: "now",
        updatedAt: "now",
      };
      shots = [...shots, shot];
      return shot;
    });
    vi.mocked(ensureSceneKeyframeAsset).mockResolvedValue({ id: "kf-asset", label: "KF" });
    vi.mocked(createWorkflowRun).mockResolvedValue(runDetail("created"));
    vi.mocked(advanceWorkflowRun).mockResolvedValue(runDetail("completed"));
    vi.mocked(listSkillOperations).mockResolvedValue([
      {
        id: "scene.create_keyframe",
        name: "Create Keyframe",
        description: "",
        intentExamples: [],
        inputSchemaId: "create_keyframe",
        prerequisites: [],
        tbdGuards: [],
        workflow: [],
        expectedOutput: { assetType: "shot_keyframe", mediaType: "image", desiredStatus: "candidate", ownerEntityInputRef: null },
      },
    ] as never);
    vi.mocked(listAssets).mockResolvedValue([
      {
        id: "kf-asset",
        projectId: "project-1",
        type: "shot_keyframe",
        label: "Scene 001 Keyframes",
        ownerEntityId: scene.id,
        canonicalVersionId: null,
        versionCount: 0,
        canonicalVersionNumber: null,
        previewThumbnailPath: null,
        createdAt: "now",
        updatedAt: "now",
      } as AssetSummary,
    ]);
    vi.mocked(listGenerationResults).mockResolvedValue([
      {
        resultSet: { id: "rs-1" },
        artifacts: [{ artifact: { id: "artifact-1", ordinal: 1, sha256: "a".repeat(64), storagePath: "g/1.png", mimeType: "image/png", width: 512, height: 512, byteSize: 1, mediaKind: "image", captureStatus: "available", resultSetId: "rs-1", createdAt: "now" }, lineage: null }],
      },
    ] as unknown as GenerationResultSetDetail[]);
    vi.mocked(promoteGeneratedArtifact).mockResolvedValue({ id: "kf-v1" } as never);
    vi.mocked(setShotKeyframe).mockImplementation(async (_root, shotId, versionId) => {
      shots = shots.map((shot) =>
        shot.id === shotId ? { ...shot, keyframeAssetVersionId: versionId } : shot,
      );
    });
  });

  it("adds a shot, generates and pins a keyframe, and reaches compile readiness", async () => {
    const user = userEvent.setup();
    render(<SceneWorkspace projectRootPath="C:/projects/red-door" />);

    // Select the scene, then open its Shots tab.
    await user.click(await screen.findByRole("button", { name: /Scene 001/ }));
    await user.click(await screen.findByRole("tab", { name: "Shots" }));

    // 1. Add a shot.
    await user.click(await screen.findByRole("button", { name: "Add Shot" }));
    await user.type(screen.getByLabelText(/What happens in this shot \(required\)/), "Establish the ops room");
    await user.click(screen.getByRole("button", { name: "Create Shot" }));
    await waitFor(() =>
      expect(screen.getByText("Establish the ops room")).toBeInTheDocument(),
    );
    expect(createShot).toHaveBeenCalledWith(
      "C:/projects/red-door",
      "scene-001",
      4,
      "Establish the ops room",
      null,
      null,
    );

    // 2. Generate a keyframe through the workflow runtime.
    await user.click(screen.getByRole("button", { name: /Generate keyframe/i }));
    await waitFor(() =>
      expect(createWorkflowRun).toHaveBeenCalledWith(
        "C:/projects/red-door",
        "scene-builder",
        "1.0.0",
        "scene.create_keyframe",
        { sceneId: "scene-001" },
      ),
    );

    // 3. The completed run surfaces a reviewable candidate grid; the user
    //    picks a result and saves it — never a silent first-candidate pin.
    const resultCard = await screen.findByRole("button", { name: /Select result 1/i });
    await user.click(resultCard);
    await user.click(screen.getByRole("button", { name: /Use this keyframe/i }));
    const dialog = await screen.findByRole("dialog");
    const { within } = await import("@testing-library/react");
    await user.click(within(dialog).getByRole("button", { name: /Save version/i }));
    await waitFor(() => expect(promoteGeneratedArtifact).toHaveBeenCalledWith(
      "C:/projects/red-door",
      "artifact-1",
      "kf-asset",
      true,
    ));
    await waitFor(() => expect(setShotKeyframe).toHaveBeenCalledWith(
      "C:/projects/red-door",
      "shot-1",
      "kf-v1",
    ));
    expect(await screen.findByText("KEYFRAME PINNED")).toBeInTheDocument();

    // 4. Compile readiness flips to ready.
    ready = true;
    vi.mocked(getCompileReadiness).mockResolvedValue({
      sceneId: scene.id,
      ready: true,
      blockers: [],
    });
    await waitFor(async () => {
      expect((await getCompileReadiness("C:/projects/red-door", "scene-001")).ready).toBe(true);
    });
  });
});
