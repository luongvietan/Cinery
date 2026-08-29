import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AssetList } from "./AssetList";
import { createAsset, listAssets } from "./api";

vi.mock("./api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `mock-asset://${path}`,
}));

const summary = {
  projectId: "project-1",
  ownerEntityId: null,
  createdAt: "2026-08-27T06:00:00Z",
  updatedAt: "2026-08-27T06:10:00Z",
};

describe("AssetList", () => {
  beforeEach(() => {
    vi.mocked(listAssets).mockReset();
    vi.mocked(createAsset).mockReset();
  });

  it("shows an empty asset state", async () => {
    vi.mocked(listAssets).mockResolvedValue([]);
    render(
      <AssetList
        projectRootPath="/projects/red-door"
        selectedAssetId={null}
        onSelectAsset={vi.fn()}
      />,
    );
    expect(await screen.findByText("No assets yet")).toBeInTheDocument();
  });

  it("shows canonical status with version label and thumbnail", async () => {
    vi.mocked(listAssets).mockResolvedValue([
      {
        id: "asset-1",
        type: "face_lock",
        label: "MARA-FACE",
        canonicalVersionId: "version-1",
        versionCount: 3,
        canonicalVersionNumber: 3,
        previewThumbnailPath: "thumbnails/asset-1/v3.webp",
        ...summary,
      },
    ]);
    render(
      <AssetList
        projectRootPath="/projects/red-door"
        selectedAssetId={null}
        onSelectAsset={vi.fn()}
      />,
    );
    expect(await screen.findByText("MARA-FACE")).toBeInTheDocument();
    expect(screen.getByText(/v003 approved/)).toBeInTheDocument();
    expect(screen.getByRole("presentation")).toBeInTheDocument();
  });

  it("shows no versions separately from no canonical version", async () => {
    vi.mocked(listAssets).mockResolvedValue([
      {
        id: "asset-2",
        type: "outfit",
        label: "MARA-OUTFIT",
        canonicalVersionId: null,
        versionCount: 0,
        canonicalVersionNumber: null,
        previewThumbnailPath: null,
        ...summary,
      },
      {
        id: "asset-3",
        type: "face_lock",
        label: "MARA-FACE-DRAFT",
        canonicalVersionId: null,
        versionCount: 2,
        canonicalVersionNumber: null,
        previewThumbnailPath: "thumbnails/asset-3/v2.webp",
        ...summary,
      },
    ]);
    render(
      <AssetList
        projectRootPath="/projects/red-door"
        selectedAssetId={null}
        onSelectAsset={vi.fn()}
      />,
    );
    expect(await screen.findByText("MARA-OUTFIT")).toBeInTheDocument();
    expect(screen.getByText(/No versions/)).toBeInTheDocument();
    expect(screen.getByText(/No approved version yet/)).toBeInTheDocument();
  });

  it("does not offer video or audio in the create-asset type selector", async () => {
    vi.mocked(listAssets).mockResolvedValue([]);
    const user = userEvent.setup();
    render(
      <AssetList
        projectRootPath="/projects/red-door"
        selectedAssetId={null}
        onSelectAsset={vi.fn()}
      />,
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
          type: "face_lock",
          label: "MARA-FACE",
          canonicalVersionId: null,
          versionCount: 0,
          canonicalVersionNumber: null,
          previewThumbnailPath: null,
          ...summary,
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
        selectedAssetId={null}
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
