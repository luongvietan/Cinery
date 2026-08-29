import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SceneWorldAssignment } from "./SceneWorldAssignment";
import { getScene, resolveSceneReferences, assignSceneWorld, upgradeSceneWorldReference } from "./api";
import { listWorldsDetailed } from "../worlds/api";

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
}));
vi.mock("../assets/api", () => ({
  getAssetWithVersions: vi.fn(),
  listAssets: vi.fn(),
}));

describe("SceneWorldAssignment", () => {
  beforeEach(() => {
    vi.mocked(getScene).mockReset();
    vi.mocked(resolveSceneReferences).mockReset();
    vi.mocked(listWorldsDetailed).mockReset();
    vi.mocked(assignSceneWorld).mockReset();
    vi.mocked(upgradeSceneWorldReference).mockReset();
  });

  it("shows world picker with current canonical World Plate", async () => {
    vi.mocked(getScene).mockResolvedValue({
      id: "scene-1",
      projectId: "p",
      ordinal: 1,
      title: "Scene",
      summary: "Summary",
      worldId: null,
      worldAssetVersionId: null,
      keyframeAssetId: null,
      createdAt: "now",
      updatedAt: "now",
    } as any);
    vi.mocked(resolveSceneReferences).mockResolvedValue({
      sceneId: "scene-1",
      world: null,
      characters: [],
      props: [],
    } as any);
    vi.mocked(listWorldsDetailed).mockResolvedValue([
      {
        world: { id: "world-1", projectId: "p", canonLocationEntityId: "loc-1", worldPlateAssetId: "asset-1", createdAt: "now", updatedAt: "now" },
        location: { id: "loc-1", projectId: "p", type: "location", name: "The Station", slug: "the-station", createdAt: "now", updatedAt: "now" },
        worldPlateAsset: { id: "asset-1", projectId: "p", type: "world_plate", label: "THE-STATION-WORLD", ownerEntityId: "world-1", canonicalVersionId: "v2", createdAt: "now", updatedAt: "now" },
      } as any,
    ]);

    render(<SceneWorldAssignment projectRootPath="/projects/red-door" sceneId="scene-1" />);

    expect(await screen.findByLabelText("World picker")).toBeInTheDocument();
    expect(screen.getByText("Current canonical World Plate:")).toBeInTheDocument();
    expect(screen.getAllByText(/THE-STATION-WORLD.*CANONICAL/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getByRole("button", { name: "Assign World" })).toBeInTheDocument();
  });

  it("after assignment shows exact version and health", async () => {
    vi.mocked(getScene).mockResolvedValue({
      id: "scene-1",
      projectId: "p",
      ordinal: 1,
      title: "Scene",
      summary: "Summary",
      worldId: "world-1",
      worldAssetVersionId: "v1",
      keyframeAssetId: null,
      createdAt: "now",
      updatedAt: "now",
    } as any);
    vi.mocked(resolveSceneReferences).mockResolvedValue({
      sceneId: "scene-1",
      world: {
        assetId: "asset-1",
        pinnedVersionId: "v1",
        currentCanonicalVersionId: "v2",
        health: "upgrade_available",
        versionNumber: 1,
        status: "superseded",
        filePath: "assets/v1/preview.png",
      },
      characters: [],
      props: [],
    } as any);
    vi.mocked(listWorldsDetailed).mockResolvedValue([
      {
        world: { id: "world-1", projectId: "p", canonLocationEntityId: "loc-1", worldPlateAssetId: "asset-1", createdAt: "now", updatedAt: "now" },
        location: { id: "loc-1", projectId: "p", type: "location", name: "The Station", slug: "the-station", createdAt: "now", updatedAt: "now" },
        worldPlateAsset: { id: "asset-1", projectId: "p", type: "world_plate", label: "THE-STATION-WORLD", ownerEntityId: "world-1", canonicalVersionId: "v2", createdAt: "now", updatedAt: "now" },
      } as any,
    ]);

    render(<SceneWorldAssignment projectRootPath="/projects/red-door" sceneId="scene-1" />);

    expect((await screen.findAllByText(/PINNED/)).length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText(/CURRENT CANONICAL.*v2/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("UPGRADE AVAILABLE").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText(/v1/).length).toBeGreaterThanOrEqual(1);
    // exact pinned version id visible
    expect(screen.getByText(/Pinned version id: v1/)).toBeInTheDocument();
  });

  it("shows staleness display with PINNED V01 CURRENT CANONICAL V02 UPGRADE AVAILABLE", async () => {
    vi.mocked(getScene).mockResolvedValue({
      id: "scene-1",
      projectId: "p",
      ordinal: 1,
      title: "Scene",
      summary: "Summary",
      worldId: "world-1",
      worldAssetVersionId: "version-01",
      keyframeAssetId: null,
      createdAt: "now",
      updatedAt: "now",
    } as any);
    vi.mocked(resolveSceneReferences).mockResolvedValue({
      sceneId: "scene-1",
      world: {
        assetId: "asset-1",
        pinnedVersionId: "version-01",
        currentCanonicalVersionId: "version-02",
        health: "upgrade_available",
        versionNumber: 1,
        status: "superseded",
        filePath: "assets/v1/preview.png",
      },
      characters: [],
      props: [],
    } as any);
    vi.mocked(listWorldsDetailed).mockResolvedValue([
      {
        world: { id: "world-1", projectId: "p", canonLocationEntityId: "loc-1", worldPlateAssetId: "asset-1", createdAt: "now", updatedAt: "now" },
        location: { id: "loc-1", projectId: "p", type: "location", name: "Station", slug: "station", createdAt: "now", updatedAt: "now" },
        worldPlateAsset: { id: "asset-1", projectId: "p", type: "world_plate", label: "STATION-WORLD", ownerEntityId: "world-1", canonicalVersionId: "version-02", createdAt: "now", updatedAt: "now" },
      } as any,
    ]);

    render(<SceneWorldAssignment projectRootPath="/projects/red-door" sceneId="scene-1" />);

    expect((await screen.findAllByText(/PINNED/)).length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText(/CURRENT CANONICAL.*version-02/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("UPGRADE AVAILABLE").length).toBeGreaterThanOrEqual(1);
  });

  it("requires explicit confirmation with Upgrade Scene to V02 and shows old/new versions", async () => {
    vi.mocked(getScene).mockResolvedValue({
      id: "scene-1",
      projectId: "p",
      ordinal: 1,
      title: "Scene",
      summary: "Summary",
      worldId: "world-1",
      worldAssetVersionId: "v1",
      keyframeAssetId: null,
      createdAt: "now",
      updatedAt: "now",
    } as any);
    vi.mocked(resolveSceneReferences).mockResolvedValue({
      sceneId: "scene-1",
      world: {
        assetId: "asset-1",
        pinnedVersionId: "v1",
        currentCanonicalVersionId: "v2",
        health: "upgrade_available",
        versionNumber: 1,
        status: "superseded",
        filePath: "assets/v1/preview.png",
      },
      characters: [],
      props: [],
    } as any);
    vi.mocked(listWorldsDetailed).mockResolvedValue([
      {
        world: { id: "world-1", projectId: "p", canonLocationEntityId: "loc-1", worldPlateAssetId: "asset-1", createdAt: "now", updatedAt: "now" },
        location: { id: "loc-1", projectId: "p", type: "location", name: "Station", slug: "station", createdAt: "now", updatedAt: "now" },
        worldPlateAsset: { id: "asset-1", projectId: "p", type: "world_plate", label: "STATION-WORLD", ownerEntityId: "world-1", canonicalVersionId: "v2", createdAt: "now", updatedAt: "now" },
      } as any,
    ]);
    vi.mocked(upgradeSceneWorldReference).mockResolvedValue({
      assetId: "asset-1",
      pinnedVersionId: "v2",
      currentCanonicalVersionId: "v2",
      health: "current",
      versionNumber: 2,
      status: "canonical",
      filePath: "assets/v2/preview.png",
    } as any);

    const user = userEvent.setup();
    render(<SceneWorldAssignment projectRootPath="/projects/red-door" sceneId="scene-1" />);

    const upgradeTrigger = await screen.findByRole("button", { name: /Upgrade Scene to V02/ });
    expect(upgradeTrigger).toBeInTheDocument();
    // visible button labels check - not generic "Update"
    expect(screen.queryByRole("button", { name: /^Update$/ })).not.toBeInTheDocument();

    await user.click(upgradeTrigger);

    // confirmation dialog should show old/new versions
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toBeInTheDocument();
    expect(screen.getAllByText(/Old version: v1/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText(/New version: v2/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText(/Scene:.*scene-1/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText(/Pinned: v1/).length).toBeGreaterThanOrEqual(1);
    expect(within(dialog).getAllByText(/Current canonical: v2/).length).toBeGreaterThanOrEqual(1);

    const confirmButton = within(dialog).getByRole("button", { name: /Upgrade Scene to V02/ });
    expect(confirmButton).toBeInTheDocument();
    await user.click(confirmButton);

    await waitFor(() => expect(upgradeSceneWorldReference).toHaveBeenCalledWith("/projects/red-door", "scene-1"));
  });

  it("does not use generic Update for upgrade action", async () => {
    vi.mocked(getScene).mockResolvedValue({
      id: "scene-1",
      projectId: "p",
      ordinal: 1,
      title: "Scene",
      summary: "Summary",
      worldId: "world-1",
      worldAssetVersionId: "v1",
      keyframeAssetId: null,
      createdAt: "now",
      updatedAt: "now",
    } as any);
    vi.mocked(resolveSceneReferences).mockResolvedValue({
      sceneId: "scene-1",
      world: {
        assetId: "asset-1",
        pinnedVersionId: "v1",
        currentCanonicalVersionId: "v2",
        health: "upgrade_available",
        versionNumber: 1,
        status: "superseded",
        filePath: "assets/v1/preview.png",
      },
      characters: [],
      props: [],
    } as any);
    vi.mocked(listWorldsDetailed).mockResolvedValue([
      {
        world: { id: "world-1", projectId: "p", canonLocationEntityId: "loc-1", worldPlateAssetId: "asset-1", createdAt: "now", updatedAt: "now" },
        location: { id: "loc-1", projectId: "p", type: "location", name: "Station", slug: "station", createdAt: "now", updatedAt: "now" },
        worldPlateAsset: { id: "asset-1", projectId: "p", type: "world_plate", label: "STATION-WORLD", ownerEntityId: "world-1", canonicalVersionId: "v2", createdAt: "now", updatedAt: "now" },
      } as any,
    ]);

    render(<SceneWorldAssignment projectRootPath="/projects/red-door" sceneId="scene-1" />);
    await screen.findByRole("button", { name: /Upgrade Scene to V02/ });
    // Ensure no generic Update button exists
    const allButtons = screen.getAllByRole("button");
    const genericUpdate = allButtons.filter((b) => b.textContent?.trim() === "Update");
    expect(genericUpdate.length).toBe(0);
  });
});
