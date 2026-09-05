import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SequenceFlow, SequenceBrief, SequencePreflight, ExtensionRequest } from "@cinematic/domain";
import { SceneWorkspace } from "../features/scenes/SceneWorkspace";
import {
  createScene,
  getScene,
  listScenes,
  resolveSceneReferences,
  listSceneCharacters,
  listSceneProps,
  listSceneTbdBindings,
  listShots,
  getCompileReadiness,
  listCinemaCompilations,
  buildSequencePreflight,
  listShotVideoCandidates,
  promoteShotVideoCandidate,
  rejectShotVideoCandidate,
  restoreShotVideoCandidate,
  resolveCanonicalShotVideo,
  type Shot,
  type ShotVideoCandidate,
} from "../features/scenes/api";
import {
  getSequenceFlow,
  updateSequenceBrief,
  markSequenceReferencesReady,
  approveSequencePreflight,
  markSequenceCanonicalTake,
  prepareSequenceExtension,
} from "../features/scenes/sequenceFlowApi";
import { createWorkflowRun, listWorkflowRuns } from "../features/workflows/api";
import { listAssets, getAssetWithVersions } from "../features/assets/api";

// This is the acceptance journey for the Joey Cinema sequence-first flow
// (Tasks 1-6): a director locks a brief, marks references ready, approves the
// generation preflight, promotes a canonical take, then prepares — but never
// executes — a sequel extension. Nothing here starts a provider workflow.

vi.mock("../features/scenes/api", () => ({
  createScene: vi.fn(),
  listScenes: vi.fn(),
  getScene: vi.fn(),
  updateSceneDetails: vi.fn(),
  assignSceneWorld: vi.fn(),
  clearSceneWorld: vi.fn(),
  addSceneCharacter: vi.fn(),
  removeSceneCharacter: vi.fn(),
  listSceneCharacters: vi.fn().mockResolvedValue([]),
  addSceneProp: vi.fn(),
  removeSceneProp: vi.fn(),
  listSceneProps: vi.fn().mockResolvedValue([]),
  resolveSceneReferences: vi.fn().mockResolvedValue({ sceneId: "scene-1", world: null, characters: [], props: [] }),
  upgradeSceneWorldReference: vi.fn(),
  upgradeSceneCharacterLookReference: vi.fn(),
  upgradeSceneCharacterSheetReference: vi.fn(),
  upgradeScenePropReference: vi.fn(),
  listShots: vi.fn(),
  createShot: vi.fn(),
  updateShot: vi.fn(),
  deleteShot: vi.fn(),
  reorderShots: vi.fn(),
  setShotKeyframe: vi.fn(),
  setShotVideo: vi.fn(),
  buildSequencePreflight: vi.fn(),
  getShotImageToVideoSource: vi.fn(),
  promoteShotVideoCandidate: vi.fn(),
  listShotVideoCandidates: vi.fn(),
  resolveCanonicalShotVideo: vi.fn(),
  rejectShotVideoCandidate: vi.fn(),
  restoreShotVideoCandidate: vi.fn(),
  setSceneTbdBinding: vi.fn(),
  removeSceneTbdBinding: vi.fn(),
  listSceneTbdBindings: vi.fn().mockResolvedValue([]),
  getCompileReadiness: vi.fn(),
  compileCinema: vi.fn(),
  listCinemaCompilations: vi.fn().mockResolvedValue([]),
  ensureSceneKeyframeAsset: vi.fn(),
}));

vi.mock("../features/scenes/sequenceFlowApi", () => ({
  getSequenceFlow: vi.fn(),
  updateSequenceBrief: vi.fn(),
  markSequenceReferencesReady: vi.fn(),
  approveSequencePreflight: vi.fn(),
  beginSequenceReview: vi.fn(),
  markSequenceCanonicalTake: vi.fn(),
  prepareSequenceExtension: vi.fn(),
}));

vi.mock("../features/workflows/api", () => ({
  advanceWorkflowRun: vi.fn(),
  approveWorkflowStep: vi.fn(),
  rejectWorkflowStep: vi.fn(),
  cancelWorkflowRun: vi.fn(),
  getWorkflowRun: vi.fn(),
  listWorkflowRuns: vi.fn().mockResolvedValue([]),
  listWorkflowCharacters: vi.fn(),
  listSkillOperations: vi.fn().mockResolvedValue([]),
  createWorkflowRun: vi.fn(),
}));

vi.mock("../features/generation/api", () => ({
  listGenerationResults: vi.fn().mockResolvedValue([]),
  promoteGeneratedArtifact: vi.fn(),
}));

vi.mock("../features/assets/api", () => ({
  getAssetWithVersions: vi.fn(),
  listAssets: vi.fn().mockResolvedValue([]),
  createAsset: vi.fn(),
  importAssetVersion: vi.fn(),
  promoteAssetVersion: vi.fn(),
}));

vi.mock("../features/worlds/api", () => ({
  listWorlds: vi.fn().mockResolvedValue([]),
  listWorldsDetailed: vi.fn().mockResolvedValue([]),
  getWorldDetailed: vi.fn().mockResolvedValue(null),
  createWorld: vi.fn(),
  getWorld: vi.fn(),
  createWorldPlateWorkflowRun: vi.fn(),
}));

vi.mock("../features/canon/api", () => ({
  getCanonEntity: vi.fn().mockResolvedValue(null),
  listCanonTbds: vi.fn().mockResolvedValue([]),
  listCanonEntities: vi.fn().mockResolvedValue([]),
  createCanonEntity: vi.fn(),
  upsertCanonSection: vi.fn(),
  lockCanonSection: vi.fn(),
  unlockCanonSection: vi.fn(),
  listCanonSectionRevisions: vi.fn(),
  createCanonTbd: vi.fn(),
  resolveCanonTbd: vi.fn(),
  reopenCanonTbd: vi.fn(),
  ensureCanonSingletons: vi.fn(),
  exportStoryBible: vi.fn(),
}));

vi.mock("../features/qa/api", () => ({
  listQaRuns: vi.fn().mockResolvedValue([]),
  getQaRun: vi.fn().mockResolvedValue(null),
}));

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `mock-asset://${path}`,
}));

const PROJECT_ROOT = "/project";
const SCENE_ID = "scene-1";

const scene = {
  id: SCENE_ID,
  projectId: "project-1",
  ordinal: 0,
  title: "Laundromat arrival",
  summary: "Tay arrives at the laundromat",
  worldId: null,
  worldAssetVersionId: null,
  keyframeAssetId: null,
  createdAt: "now",
  updatedAt: "now",
};

const shot: Shot = {
  id: "shot-1",
  sceneId: SCENE_ID,
  ordering: 0,
  durationSeconds: 8,
  keyframeAssetVersionId: null,
  generatedVideoAssetVersionId: null,
  intent: "Tay pushes through the laundromat door",
  action: null,
  camera: null,
  createdAt: "now",
  updatedAt: "now",
};

function candidate(overrides: Partial<ShotVideoCandidate> = {}): ShotVideoCandidate {
  return {
    assetVersionId: "video-take-1",
    versionNumber: 1,
    shotId: "shot-1",
    sceneId: SCENE_ID,
    createdAt: "2026-09-05T00:00:00Z",
    filePath: "assets/take-1.mp4",
    mimeType: "video/mp4",
    byteSize: 12_000,
    reviewState: "active",
    isCanonical: false,
    qaOverallStatus: null,
    qaRunCount: 0,
    providerId: "i2v",
    modelId: "motion-v1",
    workflowRunId: "run-1",
    sourceAssetVersionId: null,
    sourceKeyframeIsCurrent: true,
    ...overrides,
  };
}

const preflight: SequencePreflight = {
  sceneId: SCENE_ID,
  compilation: {
    id: "comp-1",
    projectId: "project-1",
    sceneId: SCENE_ID,
    totalDurationSeconds: 8,
    shots: [],
    behavioralLocks: {},
    worldContinuity: {},
    providerPrompt: "CINEMA PROMPT — laundromat arrival, hold on the door for 8 seconds",
    createdAt: "now",
  } as unknown as SequencePreflight["compilation"],
  providerPrompt: "CINEMA PROMPT — laundromat arrival, hold on the door for 8 seconds",
  references: [{ assetId: "asset-1", versionId: "v1", role: "world plate" }],
  estimatedCredits: 0,
  runtimeRecommendation: "Within Joey's recommended ~15-second prompt unit (8s)",
  canGenerate: true,
  blockers: [],
};

const preparedExtension: ExtensionRequest = {
  sceneId: SCENE_ID,
  shotId: "shot-1",
  direction: "sequel",
  canonicalVideoAssetVersionId: "video-take-2",
  carriedLocks: {},
  worldContinuity: {},
  continuationPrompt: "Continue immediately after the canonical take, same energy.",
};

describe("Joey sequence-first flow — full acceptance path", () => {
  let flow: SequenceFlow | null;

  beforeEach(() => {
    flow = null;

    vi.mocked(listScenes).mockResolvedValue([]);
    vi.mocked(createScene).mockResolvedValue(scene as never);
    vi.mocked(getScene).mockResolvedValue(scene as never);
    vi.mocked(resolveSceneReferences).mockResolvedValue({
      sceneId: SCENE_ID,
      world: null,
      characters: [],
      props: [],
    } as never);
    vi.mocked(listSceneCharacters).mockResolvedValue([]);
    vi.mocked(listSceneProps).mockResolvedValue([]);
    vi.mocked(listSceneTbdBindings).mockResolvedValue([]);
    vi.mocked(listShots).mockResolvedValue([shot]);
    vi.mocked(getCompileReadiness).mockResolvedValue({ sceneId: SCENE_ID, ready: true, blockers: [] });
    vi.mocked(listCinemaCompilations).mockResolvedValue([]);
    vi.mocked(buildSequencePreflight).mockResolvedValue(preflight);
    vi.mocked(listShotVideoCandidates).mockResolvedValue([
      candidate({ assetVersionId: "video-take-2", versionNumber: 2 }),
      candidate({ assetVersionId: "video-take-1", versionNumber: 1 }),
    ]);
    vi.mocked(promoteShotVideoCandidate).mockResolvedValue({
      shotId: "shot-1",
      artifactId: "video-take-2",
      assetVersionId: "video-take-2",
      previousAssetVersionId: null,
    });
    vi.mocked(rejectShotVideoCandidate).mockResolvedValue("rejected");
    vi.mocked(restoreShotVideoCandidate).mockResolvedValue("active");
    vi.mocked(resolveCanonicalShotVideo).mockResolvedValue("video-take-2");
    vi.mocked(listAssets).mockResolvedValue([]);
    vi.mocked(getAssetWithVersions).mockResolvedValue({ versions: [] } as never);
    vi.mocked(listWorkflowRuns).mockResolvedValue([]);

    // Stateful sequence-flow facade: every explicit mutation advances the one
    // persisted flow record exactly the way the Rust service enforces it.
    vi.mocked(getSequenceFlow).mockImplementation(async () => {
      if (!flow) {
        throw { code: "SEQUENCE_FLOW_NOT_FOUND", message: "No sequence flow yet" };
      }
      return flow;
    });
    vi.mocked(updateSequenceBrief).mockImplementation(async (_root, sceneId, brief: SequenceBrief) => {
      flow = {
        sceneId,
        brief,
        stage: "brief_locked",
        approvedCompilationId: null,
        canonicalShotId: null,
        extensionDirection: null,
        createdAt: flow?.createdAt ?? "now",
        updatedAt: "now-2",
      };
      return flow;
    });
    vi.mocked(markSequenceReferencesReady).mockImplementation(async () => {
      if (!flow) throw new Error("no flow to advance");
      flow = { ...flow, stage: "references_ready", updatedAt: "now-3" };
      return { flow, blockers: [] };
    });
    vi.mocked(approveSequencePreflight).mockImplementation(async (_root, _sceneId, approvedCompilationId) => {
      if (!flow) throw new Error("no flow to advance");
      flow = { ...flow, stage: "prompt_approved", approvedCompilationId: approvedCompilationId ?? null, updatedAt: "now-4" };
      return flow;
    });
    vi.mocked(markSequenceCanonicalTake).mockImplementation(async (_root, _sceneId, shotId) => {
      if (!flow) throw new Error("no flow to advance");
      flow = { ...flow, stage: "canonical_selected", canonicalShotId: shotId, updatedAt: "now-5" };
      return flow;
    });
    vi.mocked(prepareSequenceExtension).mockResolvedValue(preparedExtension);
  });

  it("guides a director from brief through a canonical take to a prepared sequel without autonomous AI actions", async () => {
    const user = userEvent.setup();
    vi.mocked(listScenes).mockResolvedValue([]);
    render(<SceneWorkspace projectRootPath={PROJECT_ROOT} />);

    // 1. Create and select the sequence.
    await user.click(await screen.findByRole("button", { name: "New Scene" }));
    await user.type(screen.getByLabelText("Title"), "Laundromat arrival");
    vi.mocked(listScenes).mockResolvedValue([scene as never]);
    await user.click(screen.getByRole("button", { name: "Create Scene" }));
    await waitFor(() => expect(createScene).toHaveBeenCalledWith(PROJECT_ROOT, "Laundromat arrival", ""));
    expect(await screen.findByRole("tab", { name: "Setup", selected: true })).toBeInTheDocument();

    // 2. Lock a valid director brief.
    await user.type(screen.getByLabelText("Creative intent"), "Tay notices the door");
    await user.click(screen.getByRole("button", { name: "Lock brief" }));
    await waitFor(() =>
      expect(updateSequenceBrief).toHaveBeenCalledWith(
        PROJECT_ROOT,
        SCENE_ID,
        expect.objectContaining({ intent: "Tay notices the door" }),
      ),
    );

    // 3. Attach the required references (world plate, cast, shots — the
    //    explicit "references ready" declaration, distinct from the brief).
    await user.click(screen.getByRole("tab", { name: "Render" }));
    await user.click(await screen.findByRole("button", { name: "Mark references ready" }));
    await waitFor(() => expect(markSequenceReferencesReady).toHaveBeenCalledWith(PROJECT_ROOT, SCENE_ID));

    // 4. Approve the generation preflight — nothing generates until this.
    const approveButton = await screen.findByRole("button", { name: "Approve generation" });
    await waitFor(() => expect(approveButton).toBeEnabled());
    await user.click(approveButton);
    await waitFor(() => expect(approveSequencePreflight).toHaveBeenCalledWith(PROJECT_ROOT, SCENE_ID, "comp-1"));

    // 5. Review the two generated candidates and promote take-2 as canonical.
    await user.click(screen.getByRole("tab", { name: "Shots" }));
    await user.click(await screen.findByRole("button", { name: "Generate video" }));
    await user.click(await screen.findByRole("button", { name: "V02" }));
    await user.click(screen.getByRole("button", { name: "Promote as canonical" }));
    await user.click(
      within(screen.getByRole("dialog")).getByRole("button", { name: "Confirm promotion" }),
    );
    await waitFor(() =>
      expect(markSequenceCanonicalTake).toHaveBeenCalledWith(PROJECT_ROOT, SCENE_ID, "shot-1"),
    );

    // 6. Choose the sequel direction and prepare — but never execute — the
    //    extension.
    await user.click(screen.getByRole("tab", { name: "Render" }));
    await user.click(await screen.findByLabelText(/After this clip \(sequel\)/));
    await user.click(await screen.findByRole("button", { name: "Prepare extension" }));

    await waitFor(() =>
      expect(prepareSequenceExtension).toHaveBeenCalledWith(PROJECT_ROOT, expect.any(String), "sequel"),
    );
    expect(createWorkflowRun).not.toHaveBeenCalled();
  });
});
