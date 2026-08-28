import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ScenePropAssignments } from "./ScenePropAssignments";
import { listSceneProps, resolveSceneReferences } from "./api";
import { listAssets, getAssetWithVersions } from "../assets/api";

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
vi.mock("../assets/api", () => ({
  listAssets: vi.fn(),
  getAssetWithVersions: vi.fn(),
  createAsset: vi.fn(),
  importAssetVersion: vi.fn(),
  promoteAssetVersion: vi.fn(),
}));
vi.mock("../canon/api", () => ({
  listCanonEntities: vi.fn(),
  listCanonTbds: vi.fn(),
}));

describe("ScenePropAssignments", () => {
  beforeEach(() => {
    vi.mocked(listSceneProps).mockReset();
    vi.mocked(resolveSceneReferences).mockReset();
    vi.mocked(listAssets).mockReset();
    vi.mocked(getAssetWithVersions).mockReset();
  });

  it("shows exact pinned version and health", async () => {
    vi.mocked(listAssets).mockResolvedValue([
      { id: "asset-prop-1", projectId: "p", type: "prop_plate", label: "PROP-A", ownerEntityId: null, canonicalVersionId: "prop-v1", createdAt: "now", updatedAt: "now", versionCount: 1, canonicalVersionNumber: 1, previewThumbnailPath: null },
    ] as any);
    vi.mocked(getAssetWithVersions).mockResolvedValue({
      asset: { id: "asset-prop-1", projectId: "p", type: "prop_plate", label: "PROP-A", ownerEntityId: null, canonicalVersionId: "prop-v1", createdAt: "now", updatedAt: "now" },
      versions: [{ id: "prop-v1", assetId: "asset-prop-1", versionNumber: 1, status: "canonical", filePath: "p", thumbnailPath: "t", sha256: "s", originalFilename: "f.png", mimeType: "image/png", byteSize: 100, width: 100, height: 100, parentVersionId: null, createdAt: "now" }],
    } as any);
    vi.mocked(listSceneProps).mockResolvedValue([
      { id: "prop-assign-1", sceneId: "scene-1", propAssetVersionId: "prop-v1", label: "Hero Prop", notes: "On table", createdAt: "now" } as any,
    ]);
    vi.mocked(resolveSceneReferences).mockResolvedValue({
      sceneId: "scene-1",
      world: null,
      characters: [],
      props: [
        {
          assignmentId: "prop-assign-1",
          reference: {
            assetId: "asset-prop-1",
            pinnedVersionId: "prop-v1",
            currentCanonicalVersionId: "prop-v2",
            health: "upgrade_available",
            versionNumber: 1,
            status: "superseded",
            filePath: "p",
          },
        },
      ],
    } as any);

    render(<ScenePropAssignments projectRootPath="/projects/red-door" sceneId="scene-1" />);

    expect(await screen.findByText("Hero Prop")).toBeInTheDocument();
    expect(screen.getAllByText(/prop-v1/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText(/Exact pinned version:/)).toBeInTheDocument();
    expect(screen.getAllByText(/PINNED/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText(/CURRENT CANONICAL.*prop-v2/)).toBeInTheDocument();
    expect(screen.getAllByText("UPGRADE_AVAILABLE").length).toBeGreaterThanOrEqual(1);
    expect(screen.getByRole("button", { name: /Upgrade Scene to V02/ })).toBeInTheDocument();
    expect(screen.getByText(/On table/)).toBeInTheDocument();
  });

  it("has visible Add Prop button and handles empty state", async () => {
    vi.mocked(listAssets).mockResolvedValue([]);
    vi.mocked(listSceneProps).mockResolvedValue([]);
    vi.mocked(resolveSceneReferences).mockResolvedValue({ sceneId: "scene-1", world: null, characters: [], props: [] } as any);

    render(<ScenePropAssignments projectRootPath="/projects/red-door" sceneId="scene-1" />);

    expect(await screen.findByText("No props assigned. Pin exact canonical prop_plate versions.")).toBeInTheDocument();
    const addButton = screen.getByRole("button", { name: "Add Prop" });
    expect(addButton).toBeVisible();
    expect(addButton.textContent).toBe("Add Prop");
  });
});
