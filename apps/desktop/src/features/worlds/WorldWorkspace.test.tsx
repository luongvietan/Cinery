import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { WorldWorkspace } from "./WorldWorkspace";
import { listWorldsDetailed, getWorldDetailed } from "./api";
import { getCanonEntity, listCanonTbds } from "../canon/api";
import { getAssetWithVersions } from "../assets/api";

vi.mock("./api", () => ({
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
vi.mock("../assets/shell", () => ({
  openAssetFolder: vi.fn(),
  openProjectRelativePath: vi.fn(),
  revealProjectRelativePath: vi.fn(),
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

describe("WorldWorkspace", () => {
  beforeEach(() => {
    vi.mocked(listWorldsDetailed).mockResolvedValue([]);
    vi.mocked(getWorldDetailed).mockReset();
    vi.mocked(getCanonEntity).mockReset();
    vi.mocked(listCanonTbds).mockReset();
    vi.mocked(getAssetWithVersions).mockReset();
  });

  it("shows empty state and allows creating a world", async () => {
    vi.mocked(listWorldsDetailed).mockResolvedValue([]);
    render(<WorldWorkspace projectRootPath="/projects/red-door" />);
    expect(await screen.findByText("No worlds yet")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "New World" })).toBeInTheDocument();
  });

  it("opens a world and shows its detail", async () => {
    vi.mocked(listWorldsDetailed).mockResolvedValue([
      {
        world: {
          id: "world-1",
          projectId: "p",
          canonLocationEntityId: "loc-1",
          worldPlateAssetId: "asset-1",
          createdAt: "now",
          updatedAt: "now",
        },
        location: {
          id: "loc-1",
          projectId: "p",
          type: "location",
          name: "The Station",
          slug: "the-station",
          createdAt: "now",
          updatedAt: "now",
        },
        worldPlateAsset: {
          id: "asset-1",
          projectId: "p",
          type: "world_plate",
          label: "THE-STATION-WORLD",
          ownerEntityId: "world-1",
          canonicalVersionId: null,
          createdAt: "now",
          updatedAt: "now",
        },
      } as any,
    ]);
    vi.mocked(getWorldDetailed).mockResolvedValue({
      world: {
        id: "world-1",
        projectId: "p",
        canonLocationEntityId: "loc-1",
        worldPlateAssetId: "asset-1",
        createdAt: "now",
        updatedAt: "now",
      },
      location: {
        id: "loc-1",
        projectId: "p",
        type: "location",
        name: "The Station",
        slug: "the-station",
        createdAt: "now",
        updatedAt: "now",
      },
      worldPlateAsset: {
        id: "asset-1",
        projectId: "p",
        type: "world_plate",
        label: "THE-STATION-WORLD",
        ownerEntityId: "world-1",
        canonicalVersionId: null,
        createdAt: "now",
        updatedAt: "now",
      },
    } as any);
    vi.mocked(getCanonEntity).mockResolvedValue({
      entity: { id: "loc-1", projectId: "p", type: "location", name: "The Station", slug: "the-station", createdAt: "now", updatedAt: "now" },
      sections: [
        { id: "s1", entityId: "loc-1", key: "description", value: { text: "desc" }, status: "locked", revision: 1, createdAt: "now", updatedAt: "now", lockedAt: "now" },
        { id: "s2", entityId: "loc-1", key: "geography", value: { text: "geo" }, status: "locked", revision: 1, createdAt: "now", updatedAt: "now", lockedAt: "now" },
      ],
    } as any);
    vi.mocked(listCanonTbds).mockResolvedValue([]);
    vi.mocked(getAssetWithVersions).mockResolvedValue({
      asset: {
        id: "asset-1",
        projectId: "p",
        type: "world_plate",
        label: "THE-STATION-WORLD",
        ownerEntityId: "world-1",
        canonicalVersionId: null,
        createdAt: "now",
        updatedAt: "now",
      },
      versions: [],
    } as any);

    const user = userEvent.setup();
    render(<WorldWorkspace projectRootPath="/projects/red-door" />);
    const worldButton = await screen.findByRole("button", { name: /The Station/ });
    await user.click(worldButton);
    expect(await screen.findByText("CANON LOCATION")).toBeInTheDocument();
    expect(await screen.findByText("WORLD PLATE")).toBeInTheDocument();
  });
});
