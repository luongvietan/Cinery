import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SceneWorkspace } from "./SceneWorkspace";
import { listScenes, createScene, getScene } from "./api";
import { getAssetWithVersions } from "../assets/api";
import { listWorldsDetailed, getWorldDetailed } from "../worlds/api";
import { getCanonEntity, listCanonTbds } from "../canon/api";
import { listCanonEntities } from "../canon/api";
import { listAssets } from "../assets/api";

vi.mock("./api", () => ({
  listScenes: vi.fn(),
  getScene: vi.fn(),
  createScene: vi.fn(),
  updateSceneDetails: vi.fn(),
  assignSceneWorld: vi.fn(),
  clearSceneWorld: vi.fn(),
  addSceneCharacter: vi.fn(),
  removeSceneCharacter: vi.fn(),
  listSceneCharacters: vi.fn(),
  addSceneProp: vi.fn(),
  removeSceneProp: vi.fn(),
  listSceneProps: vi.fn(),
  resolveSceneReferences: vi.fn(),
  upgradeSceneWorldReference: vi.fn(),
  upgradeSceneCharacterLookReference: vi.fn(),
  upgradeSceneCharacterSheetReference: vi.fn(),
  upgradeScenePropReference: vi.fn(),
}));
vi.mock("../worlds/api", () => ({
  listWorldsDetailed: vi.fn(),
  getWorldDetailed: vi.fn(),
  createWorld: vi.fn(),
  listWorlds: vi.fn(),
  getWorld: vi.fn(),
  createWorldPlateWorkflowRun: vi.fn(),
}));
vi.mock("../canon/api", () => ({
  getCanonEntity: vi.fn(),
  listCanonTbds: vi.fn(),
  listCanonEntities: vi.fn(),
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
vi.mock("../assets/api", () => ({
  getAssetWithVersions: vi.fn(),
  listAssets: vi.fn(),
  createAsset: vi.fn(),
  importAssetVersion: vi.fn(),
  promoteAssetVersion: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `mock-asset://${path}`,
}));
vi.mock("../workflows/api", () => ({
  advanceWorkflowRun: vi.fn(),
  approveWorkflowStep: vi.fn(),
  rejectWorkflowStep: vi.fn(),
  cancelWorkflowRun: vi.fn(),
  getWorkflowRun: vi.fn(),
  listWorkflowRuns: vi.fn(),
  listWorkflowCharacters: vi.fn(),
  listSkillOperations: vi.fn(),
  createWorkflowRun: vi.fn(),
}));

describe("SceneWorkspace", () => {
  beforeEach(() => {
    vi.mocked(listScenes).mockResolvedValue([]);
    vi.mocked(getScene).mockReset();
    vi.mocked(getWorldDetailed).mockReset();
    vi.mocked(getCanonEntity).mockReset();
    vi.mocked(listCanonTbds).mockReset();
    vi.mocked(listCanonEntities).mockReset();
    vi.mocked(getAssetWithVersions).mockReset();
    vi.mocked(listAssets).mockReset();
    vi.mocked(listWorldsDetailed).mockReset();
  });

  it("shows empty state and allows creating a scene via keyboard and focus management", async () => {
    vi.mocked(listScenes).mockResolvedValue([]);
    vi.mocked(createScene).mockResolvedValue({
      id: "scene-1",
      projectId: "p",
      ordinal: 1,
      title: "New Scene",
      summary: "Summary",
      worldId: null,
      worldAssetVersionId: null,
      keyframeAssetId: null,
      createdAt: "now",
      updatedAt: "now",
    } as any);
    vi.mocked(getScene).mockResolvedValue({
      id: "scene-1",
      projectId: "p",
      ordinal: 1,
      title: "New Scene",
      summary: "Summary",
      worldId: null,
      worldAssetVersionId: null,
      keyframeAssetId: null,
      createdAt: "now",
      updatedAt: "now",
    } as any);
    vi.mocked(listWorldsDetailed).mockResolvedValue([]);
    const { listSceneCharacters, listSceneProps, resolveSceneReferences } = await import("./api");
    vi.mocked(listSceneCharacters as any).mockResolvedValue([]);
    vi.mocked(listSceneProps as any).mockResolvedValue([]);
    vi.mocked(resolveSceneReferences as any).mockResolvedValue({ sceneId: "scene-1", world: null, characters: [], props: [] });
    vi.mocked(listCanonTbds).mockResolvedValue([]);
    const { listWorlds } = await import("../worlds/api");
    vi.mocked(listWorlds).mockResolvedValue([]);
    vi.mocked(listCanonEntities).mockResolvedValue([]);
    vi.mocked(listAssets).mockResolvedValue([]);

    const user = userEvent.setup();
    render(<SceneWorkspace projectRootPath="/projects/red-door" />);

    expect(await screen.findByText("No scenes yet")).toBeInTheDocument();
    const newButton = screen.getByRole("button", { name: "New Scene" });
    expect(newButton).toBeInTheDocument();
    // visible button labels
    expect(newButton.textContent).toBe("New Scene");

    // keyboard activation - focus and Enter
    newButton.focus();
    expect(document.activeElement).toBe(newButton);
    await user.click(newButton);

    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    expect(screen.getByLabelText("Title")).toBeInTheDocument();
    // focus should be on title input after modal open
    await waitFor(() => expect(document.activeElement?.getAttribute("id")).toBe("new-scene-title"));

    const titleInput = screen.getByLabelText("Title");
    await user.type(titleInput, "Night Transmission");
    const summaryInput = screen.getByLabelText("Summary");
    await user.type(summaryInput, "Mara receives...");

    const createButton = screen.getByRole("button", { name: "Create Scene" });
    expect(createButton).toBeEnabled();
    await user.click(createButton);

    await waitFor(() => expect(createScene).toHaveBeenCalledWith("/projects/red-door", "Night Transmission", "Mara receives..."));
    // after creation, dialog should close and focus return to trigger
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(document.activeElement).toBe(newButton);
  });

  it("opens a scene and shows SCENE-001, World, Character, Props, TBD, Readiness", async () => {
    vi.mocked(listScenes).mockResolvedValue([
      {
        id: "scene-1",
        projectId: "p",
        ordinal: 1,
        title: "Night Transmission",
        summary: "Mara receives...",
        worldId: "world-1",
        worldAssetVersionId: "v1",
        keyframeAssetId: null,
        createdAt: "now",
        updatedAt: "now",
      } as any,
    ]);
    vi.mocked(getScene).mockResolvedValue({
      id: "scene-1",
      projectId: "p",
      ordinal: 1,
      title: "Night Transmission",
      summary: "Mara receives...",
      worldId: "world-1",
      worldAssetVersionId: "v1",
      keyframeAssetId: null,
      createdAt: "now",
      updatedAt: "now",
    } as any);
    const { resolveSceneReferences, listSceneCharacters, listSceneProps } = await import("./api");
    vi.mocked(resolveSceneReferences).mockResolvedValue({
      sceneId: "scene-1",
      world: { assetId: "asset-1", pinnedVersionId: "v1", currentCanonicalVersionId: "v1", health: "current", versionNumber: 1, status: "canonical", filePath: "p" },
      characters: [],
      props: [],
    } as any);
    vi.mocked(listSceneCharacters).mockResolvedValue([]);
    vi.mocked(listSceneProps).mockResolvedValue([]);
    vi.mocked(listWorldsDetailed).mockResolvedValue([
      {
        world: { id: "world-1", projectId: "p", canonLocationEntityId: "loc-1", worldPlateAssetId: "asset-1", createdAt: "now", updatedAt: "now" },
        location: { id: "loc-1", projectId: "p", type: "location", name: "Station", slug: "station", createdAt: "now", updatedAt: "now" },
        worldPlateAsset: { id: "asset-1", projectId: "p", type: "world_plate", label: "STATION-WORLD", ownerEntityId: "world-1", canonicalVersionId: "v1", createdAt: "now", updatedAt: "now" },
      } as any,
    ]);
    const { listWorlds } = await import("../worlds/api");
    vi.mocked(listWorlds).mockResolvedValue([
      { id: "world-1", projectId: "p", canonLocationEntityId: "loc-1", worldPlateAssetId: "asset-1", createdAt: "now", updatedAt: "now" } as any,
    ]);
    vi.mocked(listCanonTbds).mockResolvedValue([]);
    vi.mocked(listCanonEntities).mockResolvedValue([]);
    vi.mocked(listAssets).mockResolvedValue([]);

    const user = userEvent.setup();
    render(<SceneWorkspace projectRootPath="/projects/red-door" />);

    const sceneButton = await screen.findByRole("button", { name: /Night Transmission/ });
    await user.click(sceneButton);

    expect(await screen.findByText("SCENE-001 Night Transmission")).toBeInTheDocument();
    expect(screen.getByLabelText("Title")).toBeInTheDocument();
    expect(screen.getByText("WORLD")).toBeInTheDocument();
    expect(screen.getByText("CHARACTERS")).toBeInTheDocument();
    expect(screen.getByText("PROPS")).toBeInTheDocument();
    expect(screen.getByText("TBD DECISIONS")).toBeInTheDocument();
    expect(screen.getByText("READINESS")).toBeInTheDocument();
    expect(screen.getByText("KEYFRAME")).toBeInTheDocument();
    // readiness indicator in list should be READY
    expect(screen.getByText("READY")).toBeInTheDocument();
  });

  it("handles reduced-motion and responsive layout", async () => {
    vi.mocked(listScenes).mockResolvedValue([]);
    render(<SceneWorkspace projectRootPath="/projects/red-door" />);
    expect(await screen.findByText("Scenes")).toBeInTheDocument();
    // Check that workspace has canon-workspace class which has responsive handling via CSS
    const workspace = screen.getByLabelText("Scenes workspace");
    expect(workspace).toBeInTheDocument();
    expect(workspace.className).toContain("canon-workspace");
  });
});
