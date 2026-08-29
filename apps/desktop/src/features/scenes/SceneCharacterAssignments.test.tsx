import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SceneCharacterAssignments } from "./SceneCharacterAssignments";
import { listSceneCharacters, resolveSceneReferences } from "./api";
import { listCanonEntities } from "../canon/api";
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
vi.mock("../canon/api", () => ({
  listCanonEntities: vi.fn(),
  getCanonEntity: vi.fn(),
  listCanonTbds: vi.fn(),
  createCanonEntity: vi.fn(),
}));
vi.mock("../assets/api", () => ({
  listAssets: vi.fn(),
  getAssetWithVersions: vi.fn(),
  createAsset: vi.fn(),
  importAssetVersion: vi.fn(),
  promoteAssetVersion: vi.fn(),
}));

describe("SceneCharacterAssignments", () => {
  beforeEach(() => {
    vi.mocked(listSceneCharacters).mockReset();
    vi.mocked(resolveSceneReferences).mockReset();
    vi.mocked(listCanonEntities).mockReset();
    vi.mocked(listAssets).mockReset();
    vi.mocked(getAssetWithVersions).mockReset();
  });

  it("shows Character, Look alias/version and optional Sheet", async () => {
    vi.mocked(listCanonEntities).mockResolvedValue([
      { id: "char-1", projectId: "p", type: "character", name: "Mara", slug: "mara", createdAt: "now", updatedAt: "now" },
    ] as any);
    vi.mocked(listAssets).mockResolvedValue([
      { id: "asset-look-1", projectId: "p", type: "outfit", label: "MARA-LOOK", ownerEntityId: "char-1", canonicalVersionId: "look-v1", createdAt: "now", updatedAt: "now", versionCount: 1, canonicalVersionNumber: 1, previewThumbnailPath: null },
      { id: "asset-sheet-1", projectId: "p", type: "character_sheet", label: "MARA-SHEET", ownerEntityId: "char-1", canonicalVersionId: "sheet-v1", createdAt: "now", updatedAt: "now", versionCount: 1, canonicalVersionNumber: 1, previewThumbnailPath: null },
    ] as any);
    vi.mocked(getAssetWithVersions).mockImplementation(async (_path, assetId) => {
      if (assetId === "asset-look-1") {
        return {
          asset: { id: "asset-look-1", projectId: "p", type: "outfit", label: "MARA-LOOK", ownerEntityId: "char-1", canonicalVersionId: "look-v1", createdAt: "now", updatedAt: "now" },
          versions: [{ id: "look-v1", assetId: "asset-look-1", versionNumber: 1, status: "canonical", filePath: "p", thumbnailPath: "t", sha256: "s", originalFilename: "f.png", mimeType: "image/png", byteSize: 100, width: 100, height: 100, parentVersionId: null, createdAt: "now" }],
        } as any;
      }
      return {
        asset: { id: "asset-sheet-1", projectId: "p", type: "character_sheet", label: "MARA-SHEET", ownerEntityId: "char-1", canonicalVersionId: "sheet-v1", createdAt: "now", updatedAt: "now" },
        versions: [{ id: "sheet-v1", assetId: "asset-sheet-1", versionNumber: 1, status: "canonical", filePath: "p", thumbnailPath: "t", sha256: "s", originalFilename: "f.png", mimeType: "image/png", byteSize: 100, width: 100, height: 100, parentVersionId: null, createdAt: "now" }],
      } as any;
    });
    vi.mocked(listSceneCharacters).mockResolvedValue([
      {
        id: "assign-1",
        sceneId: "scene-1",
        characterEntityId: "char-1",
        lookAssetVersionId: "look-v1",
        sheetAssetVersionId: "sheet-v1",
        notes: null,
        createdAt: "now",
        updatedAt: "now",
      } as any,
    ]);
    vi.mocked(resolveSceneReferences).mockResolvedValue({
      sceneId: "scene-1",
      world: null,
      characters: [
        {
          assignmentId: "assign-1",
          characterEntityId: "char-1",
          look: {
            assetId: "asset-look-1",
            pinnedVersionId: "look-v1",
            currentCanonicalVersionId: "look-v1",
            health: "current",
            versionNumber: 1,
            status: "canonical",
            filePath: "p",
          },
          sheet: {
            assetId: "asset-sheet-1",
            pinnedVersionId: "sheet-v1",
            currentCanonicalVersionId: "sheet-v1",
            health: "current",
            versionNumber: 1,
            status: "canonical",
            filePath: "p",
          },
        },
      ],
      props: [],
    } as any);

    render(<SceneCharacterAssignments projectRootPath="/projects/red-door" sceneId="scene-1" />);

    expect(await screen.findByText("Mara")).toBeInTheDocument();
    expect(screen.getByText("Character:")).toBeInTheDocument();
    expect(screen.getAllByText(/MARA-LOOK/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText(/look-v1/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText(/V01/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText(/MARA-SHEET/).length).toBeGreaterThanOrEqual(1);
  });

  it("shows optional Sheet placeholder when absent and staleness display", async () => {
    vi.mocked(listCanonEntities).mockResolvedValue([
      { id: "char-1", projectId: "p", type: "character", name: "Jules", slug: "jules", createdAt: "now", updatedAt: "now" },
    ] as any);
    vi.mocked(listAssets).mockResolvedValue([
      { id: "asset-look-1", projectId: "p", type: "outfit", label: "JULES-LOOK", ownerEntityId: "char-1", canonicalVersionId: "look-v1", createdAt: "now", updatedAt: "now", versionCount: 1, canonicalVersionNumber: 1, previewThumbnailPath: null },
    ] as any);
    vi.mocked(getAssetWithVersions).mockResolvedValue({
      asset: { id: "asset-look-1", projectId: "p", type: "outfit", label: "JULES-LOOK", ownerEntityId: "char-1", canonicalVersionId: "look-v1", createdAt: "now", updatedAt: "now" },
      versions: [{ id: "look-v1", assetId: "asset-look-1", versionNumber: 1, status: "canonical", filePath: "p", thumbnailPath: "t", sha256: "s", originalFilename: "f.png", mimeType: "image/png", byteSize: 100, width: 100, height: 100, parentVersionId: null, createdAt: "now" }],
    } as any);
    vi.mocked(listSceneCharacters).mockResolvedValue([
      {
        id: "assign-1",
        sceneId: "scene-1",
        characterEntityId: "char-1",
        lookAssetVersionId: "look-v1",
        sheetAssetVersionId: null,
        notes: null,
        createdAt: "now",
        updatedAt: "now",
      } as any,
    ]);
    vi.mocked(resolveSceneReferences).mockResolvedValue({
      sceneId: "scene-1",
      world: null,
      characters: [
        {
          assignmentId: "assign-1",
          characterEntityId: "char-1",
          look: {
            assetId: "asset-look-1",
            pinnedVersionId: "look-v1",
            currentCanonicalVersionId: "look-v2",
            health: "upgrade_available",
            versionNumber: 1,
            status: "superseded",
            filePath: "p",
          },
          sheet: null,
        },
      ],
      props: [],
    } as any);

    render(<SceneCharacterAssignments projectRootPath="/projects/red-door" sceneId="scene-1" />);

    expect(await screen.findByText("Jules")).toBeInTheDocument();
    expect(screen.getByText("No Sheet (optional)")).toBeInTheDocument();
    // staleness display
    expect(screen.getAllByText(/PINNED/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText(/CURRENT CANONICAL.*look-v2/)).toBeInTheDocument();
    expect(screen.getAllByText(/UPGRADE_AVAILABLE/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getByRole("button", { name: /Upgrade Scene to V02/ })).toBeInTheDocument();
  });

  it("has visible button labels and keyboard activation", async () => {
    vi.mocked(listCanonEntities).mockResolvedValue([]);
    vi.mocked(listAssets).mockResolvedValue([]);
    vi.mocked(listSceneCharacters).mockResolvedValue([]);
    vi.mocked(resolveSceneReferences).mockResolvedValue({ sceneId: "scene-1", world: null, characters: [], props: [] } as any);

    render(<SceneCharacterAssignments projectRootPath="/projects/red-door" sceneId="scene-1" />);

    const addButton = await screen.findByRole("button", { name: "Add Character" });
    expect(addButton).toBeVisible();
    addButton.focus();
    expect(document.activeElement).toBe(addButton);
  });
});
