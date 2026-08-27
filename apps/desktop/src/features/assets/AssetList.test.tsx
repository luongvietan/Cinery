import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AssetList } from "./AssetList";
import { createAsset, listAssets } from "./api";

vi.mock("./api");

describe("AssetList", () => {
  beforeEach(() => {
    vi.mocked(listAssets).mockReset();
    vi.mocked(createAsset).mockReset();
  });

  it("shows an empty asset state", async () => {
    vi.mocked(listAssets).mockResolvedValue([]);
    render(
      <AssetList projectRootPath="/projects/red-door" onSelectAsset={vi.fn()} />,
    );
    expect(await screen.findByText("No assets yet")).toBeInTheDocument();
  });

  it("shows canonical status without assuming newest is canonical", async () => {
    vi.mocked(listAssets).mockResolvedValue([
      {
        id: "asset-1",
        projectId: "project-1",
        type: "face_lock",
        label: "MARA-FACE",
        ownerEntityId: null,
        canonicalVersionId: "version-1",
        createdAt: "2026-08-27T06:00:00Z",
        updatedAt: "2026-08-27T06:10:00Z",
      },
    ]);
    render(
      <AssetList projectRootPath="/projects/red-door" onSelectAsset={vi.fn()} />,
    );
    expect(await screen.findByText("MARA-FACE")).toBeInTheDocument();
    expect(screen.getByText("Canonical set")).toBeInTheDocument();
  });

  it("shows a not-yet-canonical status for an asset with no canonical version", async () => {
    vi.mocked(listAssets).mockResolvedValue([
      {
        id: "asset-2",
        projectId: "project-1",
        type: "outfit",
        label: "MARA-OUTFIT",
        ownerEntityId: null,
        canonicalVersionId: null,
        createdAt: "2026-08-27T06:00:00Z",
        updatedAt: "2026-08-27T06:10:00Z",
      },
    ]);
    render(
      <AssetList projectRootPath="/projects/red-door" onSelectAsset={vi.fn()} />,
    );
    expect(await screen.findByText("MARA-OUTFIT")).toBeInTheDocument();
    expect(screen.getByText("No canonical version")).toBeInTheDocument();
  });

  it("does not offer video or audio in the create-asset type selector", async () => {
    vi.mocked(listAssets).mockResolvedValue([]);
    const user = userEvent.setup();
    render(
      <AssetList projectRootPath="/projects/red-door" onSelectAsset={vi.fn()} />,
    );
    await screen.findByText("No assets yet");
    await user.click(screen.getByRole("button", { name: "New Asset" }));

    const select = screen.getByLabelText("Type") as HTMLSelectElement;
    const optionValues = Array.from(select.options).map(
      (option) => option.value,
    );
    expect(optionValues).not.toContain("video");
    expect(optionValues).not.toContain("audio");
    expect(optionValues).toContain("face_lock");
  });

  it("creates an asset and auto-selects it", async () => {
    vi.mocked(listAssets)
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([
        {
          id: "asset-1",
          projectId: "project-1",
          type: "face_lock",
          label: "MARA-FACE",
          ownerEntityId: null,
          canonicalVersionId: null,
          createdAt: "2026-08-27T06:00:00Z",
          updatedAt: "2026-08-27T06:00:00Z",
        },
      ]);
    vi.mocked(createAsset).mockResolvedValue({
      id: "asset-1",
      projectId: "project-1",
      type: "face_lock",
      label: "MARA-FACE",
      ownerEntityId: null,
      canonicalVersionId: null,
      createdAt: "2026-08-27T06:00:00Z",
      updatedAt: "2026-08-27T06:00:00Z",
    });

    const onSelectAsset = vi.fn();
    const user = userEvent.setup();
    render(
      <AssetList
        projectRootPath="/projects/red-door"
        onSelectAsset={onSelectAsset}
      />,
    );
    await screen.findByText("No assets yet");
    await user.click(screen.getByRole("button", { name: "New Asset" }));
    await user.type(screen.getByLabelText("Label"), "MARA-FACE");
    await user.click(screen.getByRole("button", { name: "Create" }));

    expect(await screen.findByText("MARA-FACE")).toBeInTheDocument();
    expect(createAsset).toHaveBeenCalledWith({
      projectRootPath: "/projects/red-door",
      type: "face_lock",
      label: "MARA-FACE",
    });
    expect(onSelectAsset).toHaveBeenCalledWith("asset-1");
  });
});
