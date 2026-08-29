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
import { listScenes } from "../features/cinema/api";
import { getProjectOverview } from "../features/overview/api";
import { getProjectHealth } from "../features/overview/healthApi";
import { getDiagnosticsFolder } from "../features/diagnostics/api";

vi.mock("../features/assets/api");
vi.mock("../features/workflows/api");
vi.mock("../features/cinema/api");
vi.mock("../features/overview/api");
vi.mock("../features/overview/healthApi");
vi.mock("../features/diagnostics/api");
vi.mock("../features/canon/api", () => ({
  listCanonEntities: vi.fn().mockResolvedValue([]),
  listCanonTbds: vi.fn().mockResolvedValue([]),
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
        title: "Scene 001",
        worldAssetVersionId: "world-v1",
        canonNotes: null,
        createdAt: "2026-08-28T06:00:00Z",
        updatedAt: "2026-08-28T06:00:00Z",
      },
    ]);
    // The cinema workspace auto-selects the action's scene and loads its
    // detail/readiness through the same mocked command facade.
    const { getScene, getSceneReadiness } = await import("../features/cinema/api");
    vi.mocked(getScene).mockReset().mockResolvedValue({
      scene: {
        id: "scene-001", projectId: "mara-project", title: "Scene 001", worldAssetVersionId: "world-v1",
        canonNotes: null, createdAt: "2026-08-28T06:00:00Z", updatedAt: "2026-08-28T06:00:00Z",
      },
      characters: [],
      props: [],
      shots: [{ id: "shot-1", sceneId: "scene-001", ordering: 0, durationSeconds: 4, keyframeAssetVersionId: null, intent: "Establish", action: null, camera: null, generatedVideoAssetVersionId: null, createdAt: "now", updatedAt: "now" }],
    });
    vi.mocked(getSceneReadiness).mockReset().mockResolvedValue({ sceneId: "scene-001", ready: true, blockers: [] });
    vi.mocked(getProjectOverview).mockReset().mockResolvedValue({
      readiness: {
        status: "pending",
        nextAction: {
          id: "cinema_compilation",
          title: "Cinema Compilation",
          destination: "cinema",
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
      "Providers",
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

  it("navigates the overview's continue action to the cinema panel", async () => {
    const user = userEvent.setup();
    render(<ProjectWorkspace project={project} onCloseProject={vi.fn()} />);

    const continueButton = await screen.findByRole("button", {
      name: /Continue with Cinema Compilation/,
    });
    await user.click(continueButton);

    expect(
      await screen.findByRole("button", { name: "Compile Scene 001" }),
    ).toBeInTheDocument();
  });
});
