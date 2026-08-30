import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SceneReadinessPanel } from "./SceneReadinessPanel";
import { getScene, resolveSceneReferences, listSceneCharacters, listSceneTbdBindings } from "./api";
import { listCanonTbds } from "../canon/api";
import { listWorlds } from "../worlds/api";

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
  setSceneTbdBinding: vi.fn(),
  removeSceneTbdBinding: vi.fn(),
  listSceneTbdBindings: vi.fn(),
}));
vi.mock("../canon/api", () => ({
  listCanonTbds: vi.fn(),
  listCanonEntities: vi.fn(),
}));
vi.mock("../worlds/api", () => ({
  listWorlds: vi.fn(),
  listWorldsDetailed: vi.fn(),
}));
vi.mock("../assets/api", () => ({
  listAssets: vi.fn(),
  getAssetWithVersions: vi.fn(),
}));

describe("SceneReadinessPanel", () => {
  beforeEach(() => {
    vi.mocked(getScene).mockReset();
    vi.mocked(resolveSceneReferences).mockReset();
    vi.mocked(listCanonTbds).mockReset();
    vi.mocked(listWorlds).mockReset();
    vi.mocked(listSceneCharacters).mockReset();
    vi.mocked(listSceneCharacters).mockResolvedValue([]);
    vi.mocked(listWorlds).mockResolvedValue([]);
    vi.mocked(listCanonTbds).mockResolvedValue([]);
    vi.mocked(resolveSceneReferences).mockResolvedValue({ sceneId: "scene-1", world: null, characters: [], props: [] } as any);
    vi.mocked(listSceneTbdBindings).mockReset().mockResolvedValue([]);
  });

  it("shows NOT READY with blockers and READY FOR KEYFRAME when complete", async () => {
    vi.mocked(getScene).mockResolvedValue({
      id: "scene-1",
      projectId: "p",
      ordinal: 1,
      title: "",
      summary: "",
      worldId: null,
      worldAssetVersionId: null,
      keyframeAssetId: null,
      createdAt: "now",
      updatedAt: "now",
    } as any);
    vi.mocked(resolveSceneReferences).mockResolvedValue({ sceneId: "scene-1", world: null, characters: [], props: [] } as any);
    vi.mocked(listCanonTbds).mockResolvedValue([]);
    vi.mocked(listWorlds).mockResolvedValue([]);

    const { rerender } = render(<SceneReadinessPanel projectRootPath="/projects/red-door" sceneId="scene-1" />);

    expect(await screen.findByText("NOT READY")).toBeInTheDocument();
    expect(screen.getByText(/title_missing/)).toBeInTheDocument();
    expect(screen.getByText(/summary_missing/)).toBeInTheDocument();
    expect(screen.getByText(/world_reference_missing/)).toBeInTheDocument();

    // now ready
    vi.mocked(getScene).mockResolvedValue({
      id: "scene-1",
      projectId: "p",
      ordinal: 1,
      title: "Night Transmission",
      summary: "Summary",
      worldId: "world-1",
      worldAssetVersionId: "v1",
      keyframeAssetId: null,
      createdAt: "now",
      updatedAt: "now",
    } as any);
    vi.mocked(resolveSceneReferences).mockResolvedValue({
      sceneId: "scene-1",
      world: { assetId: "a", pinnedVersionId: "v1", currentCanonicalVersionId: "v1", health: "current", versionNumber: 1, status: "canonical", filePath: "p" },
      characters: [],
      props: [],
    } as any);
    vi.mocked(listCanonTbds).mockResolvedValue([]);

    rerender(<SceneReadinessPanel projectRootPath="/projects/red-door" sceneId="scene-1" refreshKey={1} />);

    expect(await screen.findByText("READY FOR KEYFRAME")).toBeInTheDocument();
    expect(screen.getByText("No blockers — Scene can generate a keyframe.")).toBeInTheDocument();
  });

  it("shows BROKEN blocker and UPGRADE_AVAILABLE warning", async () => {
    vi.mocked(getScene).mockResolvedValue({
      id: "scene-1",
      projectId: "p",
      ordinal: 1,
      title: "Title",
      summary: "Summary",
      worldId: "world-1",
      worldAssetVersionId: "v1",
      keyframeAssetId: null,
      createdAt: "now",
      updatedAt: "now",
    } as any);
    vi.mocked(resolveSceneReferences).mockResolvedValue({
      sceneId: "scene-1",
      world: { assetId: "a", pinnedVersionId: "v1", currentCanonicalVersionId: "v2", health: "broken", versionNumber: 1, status: "canonical", filePath: "p" },
      characters: [],
      props: [],
    } as any);
    vi.mocked(listCanonTbds).mockResolvedValue([]);
    vi.mocked(listWorlds).mockResolvedValue([]);

    render(<SceneReadinessPanel projectRootPath="/projects/red-door" sceneId="scene-1" />);

    expect(await screen.findByText("NOT READY")).toBeInTheDocument();
    expect(screen.getByText(/world_reference_broken/)).toBeInTheDocument();

    // upgrade_available warning
    vi.mocked(resolveSceneReferences).mockResolvedValue({
      sceneId: "scene-1",
      world: { assetId: "a", pinnedVersionId: "v1", currentCanonicalVersionId: "v2", health: "upgrade_available", versionNumber: 1, status: "superseded", filePath: "p" },
      characters: [],
      props: [],
    } as any);
    vi.mocked(getScene).mockResolvedValue({
      id: "scene-1",
      projectId: "p",
      ordinal: 1,
      title: "Title",
      summary: "Summary",
      worldId: "world-1",
      worldAssetVersionId: "v1",
      keyframeAssetId: null,
      createdAt: "now",
      updatedAt: "now",
    } as any);

    const { rerender } = render(<SceneReadinessPanel projectRootPath="/projects/red-door" sceneId="scene-1" />);
    // need to trigger refresh
    rerender(<SceneReadinessPanel projectRootPath="/projects/red-door" sceneId="scene-1" refreshKey={2} />);
    expect(await screen.findByText(/upgrade_available/)).toBeInTheDocument();
    expect(screen.getByText("READY FOR KEYFRAME")).toBeInTheDocument();
  });
});
