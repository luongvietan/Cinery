import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ProjectSummary } from "@cinematic/domain";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProjectWorkspace } from "../features/projects/ProjectWorkspace";
import { getAssetWithVersions, listAssets } from "../features/assets/api";
import {
  getProviderCapabilities,
  getProviderConfigurationStatus,
  listProviderModels,
  listProviders,
  listSkillOperations,
  listWorkflowRuns,
} from "../features/workflows/api";
import { listScenes, getCompileReadiness, listShots } from "../features/scenes/api";
import { getProjectOverview } from "../features/overview/api";
import { getProjectHealth } from "../features/overview/healthApi";
import { getDiagnosticsFolder } from "../features/diagnostics/api";

vi.mock("../features/assets/api");
vi.mock("../features/workflows/api");
vi.mock("../features/scenes/api", () => ({
  listScenes: vi.fn().mockResolvedValue([]),
  getScene: vi.fn().mockResolvedValue(null),
  resolveSceneReferences: vi.fn().mockResolvedValue({
    sceneId: "scene-001",
    world: null,
    characters: [],
    props: [],
  }),
  listSceneCharacters: vi.fn().mockResolvedValue([]),
  listSceneProps: vi.fn().mockResolvedValue([]),
  listSceneTbdBindings: vi.fn().mockResolvedValue([]),
  removeSceneTbdBinding: vi.fn().mockResolvedValue(undefined),
  setSceneTbdBinding: vi.fn().mockResolvedValue(null),
  listShots: vi.fn().mockResolvedValue([
    { id: "shot-1", sceneId: "scene-001", ordering: 0, durationSeconds: 4, keyframeAssetVersionId: null, intent: "Establish", action: null, camera: null, createdAt: "now", updatedAt: "now" },
  ]),
  getCompileReadiness: vi.fn().mockResolvedValue({ sceneId: "scene-001", ready: true, blockers: [] }),
  listCinemaCompilations: vi.fn().mockResolvedValue([]),
}));
vi.mock("../features/overview/api");
vi.mock("../features/overview/healthApi");
vi.mock("../features/diagnostics/api");
vi.mock("../features/canon/api", () => ({
  listCanonEntities: vi.fn().mockResolvedValue([]),
  listCanonTbds: vi.fn().mockResolvedValue([]),
}));
vi.mock("../features/worlds/api", () => ({
  listWorlds: vi.fn().mockResolvedValue([]),
  listWorldsDetailed: vi.fn().mockResolvedValue([]),
  getWorldDetailed: vi.fn().mockResolvedValue(null),
}));
vi.mock("../features/qa/api", () => ({
  listQaRuns: vi.fn().mockResolvedValue([]),
  getQaRun: vi.fn().mockResolvedValue(null),
}));
vi.mock("../features/jobs/JobsPanel", () => ({
  JobsPanel: () => null,
}));
vi.mock("../features/production/api", () => ({
  routeProductionIntent: vi.fn().mockResolvedValue(null),
  listGenerationResults: vi.fn().mockResolvedValue([]),
  getGeneratedArtifact: vi.fn().mockResolvedValue(null),
  promoteGeneratedArtifact: vi.fn(),
}));
vi.mock("../features/canon/healthApi", () => ({
  getProjectHealth: vi.fn().mockResolvedValue([]),
}));
vi.mock("../features/assets/shell", () => ({
  openAssetFolder: vi.fn(),
  openProjectRelativePath: vi.fn(),
  revealProjectRelativePath: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `mock-asset://${path}`,
}));

const project: ProjectSummary = {
  id: "mara-project",
  name: "Mara MVP",
  rootPath: "/projects/mara-mvp",
  schemaVersion: 1,
  createdAt: "2026-08-28T06:00:00Z",
  updatedAt: "2026-08-28T06:00:00Z",
};

describe("MVP golden path (deterministic UI portions)", () => {
  beforeEach(async () => {
    vi.mocked(listAssets).mockReset().mockResolvedValue([
      {
        id: "face-asset",
        projectId: "mara-project",
        type: "face_lock",
        label: "Mara Face",
        ownerEntityId: "mara",
        canonicalVersionId: "face-v1",
        versionCount: 1,
        canonicalVersionNumber: 1,
        previewThumbnailPath: null,
        createdAt: "2026-08-28T06:00:00Z",
        updatedAt: "2026-08-28T06:00:00Z",
      },
    ]);
    vi.mocked(getAssetWithVersions).mockReset().mockResolvedValue({
      asset: {
        id: "face-asset",
        projectId: "mara-project",
        type: "face_lock",
        label: "Mara Face",
        ownerEntityId: "mara",
        canonicalVersionId: "face-v1",
        createdAt: "2026-08-28T06:00:00Z",
        updatedAt: "2026-08-28T06:00:00Z",
      },
      versions: [],
    });
    vi.mocked(listSkillOperations).mockReset().mockResolvedValue([]);
    vi.mocked(listWorkflowRuns).mockReset().mockResolvedValue([]);
    vi.mocked(listProviders).mockReset().mockResolvedValue(["mock"]);
    vi.mocked(getProviderCapabilities).mockReset().mockResolvedValue({
      mediaTypes: ["image"],
      supportsSeed: false,
      supportsNegativePrompt: false,
      supportsReferenceImage: true,
      supportsImageEdit: true,
      supportsMultipleReferenceImages: false,
      supportsImageToVideo: false,
      supportsCancel: false,
      supportsProgress: false,
      supportedAspectRatios: ["1:1"],
      supportedModels: ["mock-image-v1"],
    });
    vi.mocked(listProviderModels).mockReset().mockResolvedValue(["mock-image-v1"]);
    vi.mocked(getProviderConfigurationStatus).mockReset().mockResolvedValue({
      providerId: "mock",
      enabled: true,
      credentialConfigured: false,
      models: ["mock-image-v1"],
      defaultModel: "mock-image-v1",
    });
    vi.mocked(listScenes).mockReset().mockResolvedValue([
      {
        id: "scene-001",
        projectId: "mara-project",
        ordinal: 0,
        title: "Scene 001",
        summary: "Mara returns to the station",
        worldId: "world-1",
        worldAssetVersionId: "world-v1",
        keyframeAssetId: "kf-asset",
        createdAt: "2026-08-28T06:00:00Z",
        updatedAt: "2026-08-28T06:00:00Z",
      },
    ]);
    // The unified Scene workspace auto-selects the action's scene and shows
    // its shots + compile sections (readiness via the compile command).
    vi.mocked(getCompileReadiness).mockReset().mockResolvedValue({
      sceneId: "scene-001",
      ready: true,
      blockers: [],
    });
    vi.mocked(listShots).mockReset().mockResolvedValue([
      { id: "shot-1", sceneId: "scene-001", ordering: 0, durationSeconds: 4, keyframeAssetVersionId: null, intent: "Establish", action: null, camera: null, createdAt: "now", updatedAt: "now" },
    ]);
    vi.mocked(getProjectOverview).mockReset().mockResolvedValue({
      readiness: {
        status: "pending",
        nextAction: {
          id: "cinema_compilation",
          title: "Cinema Compilation",
          destination: "scenes",
          characterEntityId: null,
          sceneId: "scene-001",
        },
        steps: [],
      },
      healthSummary: {
        openProtectedTbdCount: 0,
        openTbdCount: 0,
        activeJobCount: 0,
      },
      recentActivity: [],
      activeJobs: [],
      sceneReadiness: [],
    });
    vi.mocked(getProjectHealth).mockReset().mockResolvedValue([]);
    vi.mocked(getDiagnosticsFolder).mockReset().mockResolvedValue("/projects/mara-mvp/diagnostics");
  });

  it("exposes every top-level route through keyboard navigation", async () => {
    const user = userEvent.setup();
    render(<ProjectWorkspace project={project} onCloseProject={vi.fn()} />);

    for (const route of [
      "Overview",
      "Assets",
      "Canon",
      "Workflows",
      "Production",
      "AI Services",
      "Diagnostics",
    ]) {
      await user.click(screen.getByRole("button", { name: route }));
      const navButton = screen.getByRole("button", { name: route });
      expect(navButton).toHaveAttribute("aria-pressed", "true");
    }
  });

  it("surfaces the blocked-action explanation pattern in the overview", async () => {
    vi.mocked(getProjectHealth).mockResolvedValue([
      {
        code: "MISSING_SCENE_WORLD_REFERENCE",
        severity: "error",
        entityType: "scene",
        entityId: "scene-001",
        message: "Scene references missing World AssetVersion world-v1.",
        remediation: "Choose an existing exact World version.",
      },
    ]);

    render(<ProjectWorkspace project={project} onCloseProject={vi.fn()} />);

    expect(
      await screen.findByText(/MISSING_SCENE_WORLD_REFERENCE/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Choose an existing exact World version\./),
    ).toBeInTheDocument();
  });

  it("navigates the overview's continue action to the unified Scene workspace", async () => {
    const user = userEvent.setup();
    render(<ProjectWorkspace project={project} onCloseProject={vi.fn()} />);

    const continueButton = await screen.findByRole("button", {
      name: "Continue: Compile the final prompt",
    });
    await user.click(continueButton);

    // The Scenes panel opens with the action's scene selected and its
    // compile section visible — one Scene workspace, no separate cinema view.
    expect(
      await screen.findByRole("button", { name: "Compile Scene" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Scene 001")).toBeInTheDocument();
  });
});
