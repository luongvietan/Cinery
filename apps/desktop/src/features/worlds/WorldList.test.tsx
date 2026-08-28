import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { WorldList } from "./WorldList";
import { listWorldsDetailed } from "./api";

vi.mock("./api");

const baseLocation = {
  id: "loc-1",
  projectId: "project-1",
  type: "location" as const,
  name: "The Station",
  slug: "the-station",
  createdAt: "2026-08-28T00:00:00Z",
  updatedAt: "2026-08-28T00:00:00Z",
};

const baseAssetNoCanonical = {
  id: "asset-world-1",
  projectId: "project-1",
  type: "world_plate" as const,
  label: "THE-STATION-WORLD",
  ownerEntityId: "world-1",
  canonicalVersionId: null,
  createdAt: "2026-08-28T00:00:00Z",
  updatedAt: "2026-08-28T00:00:00Z",
};

const baseAssetCanonical = {
  ...baseAssetNoCanonical,
  id: "asset-world-2",
  label: "ROOFTOP-WORLD",
  canonicalVersionId: "version-1",
};

function worldWithAsset(overrides?: Partial<typeof baseAssetCanonical>, locationOverrides?: Partial<typeof baseLocation>) {
  const location = { ...baseLocation, ...locationOverrides };
  const asset = { ...baseAssetNoCanonical, ...overrides } as typeof baseAssetCanonical;
  return {
    world: {
      id: `world-${location.id}`,
      projectId: "project-1",
      canonLocationEntityId: location.id,
      worldPlateAssetId: asset.id,
      createdAt: "2026-08-28T00:00:00Z",
      updatedAt: "2026-08-28T00:00:00Z",
    },
    location,
    worldPlateAsset: asset,
  };
}

describe("WorldList", () => {
  beforeEach(() => {
    vi.mocked(listWorldsDetailed).mockReset();
  });

  it("shows empty state", async () => {
    vi.mocked(listWorldsDetailed).mockResolvedValue([]);
    render(
      <WorldList
        projectRootPath="/projects/red-door"
        selectedWorldId={null}
        onSelectWorld={vi.fn()}
      />,
    );
    expect(await screen.findByText("No worlds yet")).toBeInTheDocument();
    expect(screen.getByText("Create a World from a Canon Location to begin.")).toBeInTheDocument();
  });

  it("shows world with no canonical Plate", async () => {
    vi.mocked(listWorldsDetailed).mockResolvedValue([
      worldWithAsset(undefined, { id: "loc-1", name: "Rooftop", slug: "rooftop" }),
    ]);
    render(
      <WorldList
        projectRootPath="/projects/red-door"
        selectedWorldId={null}
        onSelectWorld={vi.fn()}
      />,
    );
    expect(await screen.findByText("Rooftop")).toBeInTheDocument();
    expect(screen.getByText("NO WORLD PLATE YET")).toBeInTheDocument();
    expect(screen.getByText("No world plate yet")).toBeInTheDocument();
  });

  it("shows world with canonical Plate", async () => {
    vi.mocked(listWorldsDetailed).mockResolvedValue([
      worldWithAsset({ canonicalVersionId: "version-1", label: "THE-STATION-WORLD" }, { id: "loc-2", name: "The Station", slug: "the-station" }),
      // also ensure canonical asset shows label + CANONICAL
    ]);
    render(
      <WorldList
        projectRootPath="/projects/red-door"
        selectedWorldId={null}
        onSelectWorld={vi.fn()}
      />,
    );
    expect(await screen.findByText("The Station")).toBeInTheDocument();
    expect(screen.getByText("THE-STATION-WORLD · CANONICAL")).toBeInTheDocument();
    expect(screen.getByText(/World plate canonical/)).toBeInTheDocument();
  });
});
