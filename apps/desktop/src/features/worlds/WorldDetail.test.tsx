import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { WorldDetail } from "./WorldDetail";
import { getWorldDetailed } from "./api";
import { getCanonEntity, listCanonTbds } from "../canon/api";
import { getAssetWithVersions } from "../assets/api";

vi.mock("./api", () => ({
  getWorldDetailed: vi.fn(),
  createWorldPlateWorkflowRun: vi.fn(),
  listWorldsDetailed: vi.fn(),
  createWorld: vi.fn(),
  listWorlds: vi.fn(),
  getWorld: vi.fn(),
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
vi.mock("../workflows/api", () => ({
  advanceWorkflowRun: vi.fn(),
  approveWorkflowStep: vi.fn(),
  rejectWorkflowStep: vi.fn(),
  cancelWorkflowRun: vi.fn(),
  createWorkflowRun: vi.fn(),
  getWorkflowRun: vi.fn(),
  listWorkflowRuns: vi.fn(),
  listWorkflowCharacters: vi.fn(),
  listSkillOperations: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `mock-asset://${path}`,
}));
vi.mock("../assets/shell", () => ({
  openAssetFolder: vi.fn(),
  openProjectRelativePath: vi.fn(),
  revealProjectRelativePath: vi.fn(),
}));

describe("WorldDetail", () => {
  beforeEach(() => {
    vi.mocked(getWorldDetailed).mockReset();
    vi.mocked(getCanonEntity).mockReset();
    vi.mocked(listCanonTbds).mockReset();
    vi.mocked(getAssetWithVersions).mockReset();
  });

  it("shows Location lock state and world plate preview", async () => {
    vi.mocked(getWorldDetailed).mockResolvedValue({
      world: {
        id: "world-1",
        projectId: "project-1",
        canonLocationEntityId: "loc-1",
        worldPlateAssetId: "asset-1",
        createdAt: "now",
        updatedAt: "now",
      },
      location: {
        id: "loc-1",
        projectId: "project-1",
        type: "location",
        name: "The Station",
        slug: "the-station",
        createdAt: "now",
        updatedAt: "now",
      },
      worldPlateAsset: {
        id: "asset-1",
        projectId: "project-1",
        type: "world_plate",
        label: "THE-STATION-WORLD",
        ownerEntityId: "world-1",
        canonicalVersionId: "v2",
        createdAt: "now",
        updatedAt: "now",
      },
    } as any);
    vi.mocked(getCanonEntity).mockResolvedValue({
      entity: {
        id: "loc-1",
        projectId: "project-1",
        type: "location",
        name: "The Station",
        slug: "the-station",
        createdAt: "now",
        updatedAt: "now",
      },
      sections: [
        { id: "s1", entityId: "loc-1", key: "description", value: { text: "A station" }, status: "locked", revision: 1, createdAt: "now", updatedAt: "now", lockedAt: "now" },
        { id: "s2", entityId: "loc-1", key: "geography", value: { text: "Rust" }, status: "locked", revision: 1, createdAt: "now", updatedAt: "now", lockedAt: "now" },
        { id: "s3", entityId: "loc-1", key: "visual_tags", value: { tags: ["neon"] }, status: "locked", revision: 1, createdAt: "now", updatedAt: "now", lockedAt: "now" },
        { id: "s4", entityId: "loc-1", key: "rules", value: { rules: ["no entry"] }, status: "locked", revision: 1, createdAt: "now", updatedAt: "now", lockedAt: "now" },
      ],
    } as any);
    vi.mocked(listCanonTbds).mockResolvedValue([]);
    vi.mocked(getAssetWithVersions).mockResolvedValue({
      asset: {
        id: "asset-1",
        projectId: "project-1",
        type: "world_plate",
        label: "THE-STATION-WORLD",
        ownerEntityId: "world-1",
        canonicalVersionId: "v2",
        createdAt: "now",
        updatedAt: "now",
      },
      versions: [
        {
          id: "v2",
          assetId: "asset-1",
          versionNumber: 2,
          status: "canonical",
          filePath: "assets/asset-1/v002/preview.png",
          thumbnailPath: "thumbnails/asset-1/v2.webp",
          sha256: "abc",
          originalFilename: "plate.png",
          mimeType: "image/png",
          byteSize: 100,
          width: 1024,
          height: 1024,
          parentVersionId: null,
          createdAt: "now",
        },
      ],
    } as any);

    render(<WorldDetail projectRootPath="/projects/red-door" worldId="world-1" />);

    expect(await screen.findByText("The Station")).toBeInTheDocument();
    expect(screen.getByText("CANON LOCATION")).toBeInTheDocument();
    expect(screen.getAllByText("LOCKED").length).toBeGreaterThanOrEqual(2);
    expect(await screen.findByText("WORLD PLATE")).toBeInTheDocument();
    expect(await screen.findByAltText(/THE-STATION-WORLD v002 preview/)).toBeInTheDocument();
    expect(await screen.findByText("Generate Candidate")).toBeInTheDocument();
    expect(screen.getByText("View Versions")).toBeInTheDocument();
    expect(screen.getByText("Protected Unknowns")).toBeInTheDocument();
  });

  it("shows no world plate yet when canonical is missing", async () => {
    vi.mocked(getWorldDetailed).mockResolvedValue({
      world: {
        id: "world-1",
        projectId: "project-1",
        canonLocationEntityId: "loc-1",
        worldPlateAssetId: "asset-1",
        createdAt: "now",
        updatedAt: "now",
      },
      location: {
        id: "loc-1",
        projectId: "project-1",
        type: "location",
        name: "Rooftop",
        slug: "rooftop",
        createdAt: "now",
        updatedAt: "now",
      },
      worldPlateAsset: {
        id: "asset-1",
        projectId: "project-1",
        type: "world_plate",
        label: "ROOFTOP-WORLD",
        ownerEntityId: "world-1",
        canonicalVersionId: null,
        createdAt: "now",
        updatedAt: "now",
      },
    } as any);
    vi.mocked(getCanonEntity).mockResolvedValue({
      entity: { id: "loc-1", projectId: "project-1", type: "location", name: "Rooftop", slug: "rooftop", createdAt: "now", updatedAt: "now" },
      sections: [
        { id: "s1", entityId: "loc-1", key: "description", value: { text: "desc" }, status: "locked", revision: 1, createdAt: "now", updatedAt: "now", lockedAt: "now" },
        { id: "s2", entityId: "loc-1", key: "geography", value: { text: "geo" }, status: "draft", revision: 1, createdAt: "now", updatedAt: "now", lockedAt: null },
      ],
    } as any);
    vi.mocked(listCanonTbds).mockResolvedValue([]);
    vi.mocked(getAssetWithVersions).mockResolvedValue({
      asset: {
        id: "asset-1",
        projectId: "project-1",
        type: "world_plate",
        label: "ROOFTOP-WORLD",
        ownerEntityId: "world-1",
        canonicalVersionId: null,
        createdAt: "now",
        updatedAt: "now",
      },
      versions: [],
    } as any);

    render(<WorldDetail projectRootPath="/projects/red-door" worldId="world-1" />);

    expect(await screen.findByText("Rooftop")).toBeInTheDocument();
    expect(await screen.findByText(/NO WORLD PLATE YET/)).toBeInTheDocument();
  });
});
