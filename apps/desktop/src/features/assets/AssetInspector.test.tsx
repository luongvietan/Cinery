import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AssetInspector } from "./AssetInspector";
import { getAssetWithVersions } from "./api";

vi.mock("./api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `mock-asset://${path}`,
}));

describe("AssetInspector", () => {
  beforeEach(() => {
    vi.mocked(getAssetWithVersions).mockReset();
  });

  it("renders newest version first but marks canonical explicitly", async () => {
    vi.mocked(getAssetWithVersions).mockResolvedValue({
      asset: {
        id: "asset-1",
        projectId: "project-1",
        type: "face_lock",
        label: "MARA-FACE",
        ownerEntityId: null,
        canonicalVersionId: "v1",
        createdAt: "2026-08-27T06:00:00Z",
        updatedAt: "2026-08-27T06:10:00Z",
      },
      versions: [
        {
          id: "v2",
          assetId: "asset-1",
          versionNumber: 2,
          status: "candidate",
          filePath: "assets/asset-1/v002/v2.png",
          thumbnailPath: "thumbnails/asset-1/v2.webp",
          sha256: "hash2",
          originalFilename: "second.png",
          mimeType: "image/png",
          byteSize: 100,
          parentVersionId: null,
          createdAt: "2026-08-27T06:10:00Z",
        },
        {
          id: "v1",
          assetId: "asset-1",
          versionNumber: 1,
          status: "canonical",
          filePath: "assets/asset-1/v001/v1.png",
          thumbnailPath: "thumbnails/asset-1/v1.webp",
          sha256: "hash1",
          originalFilename: "first.png",
          mimeType: "image/png",
          byteSize: 100,
          parentVersionId: null,
          createdAt: "2026-08-27T06:05:00Z",
        },
      ],
    });

    render(
      <AssetInspector projectRootPath="/projects/red-door" assetId="asset-1" />,
    );

    const versions = await screen.findAllByTestId("asset-version");
    expect(versions[0]).toHaveTextContent("V02");
    expect(versions[0]).toHaveTextContent("Candidate");
    expect(versions[1]).toHaveTextContent("V01");
    expect(versions[1]).toHaveTextContent("Canonical");
  });

  it("shows immutable metadata for each version", async () => {
    vi.mocked(getAssetWithVersions).mockResolvedValue({
      asset: {
        id: "asset-1",
        projectId: "project-1",
        type: "face_lock",
        label: "MARA-FACE",
        ownerEntityId: null,
        canonicalVersionId: null,
        createdAt: "2026-08-27T06:00:00Z",
        updatedAt: "2026-08-27T06:00:00Z",
      },
      versions: [
        {
          id: "v1",
          assetId: "asset-1",
          versionNumber: 1,
          status: "candidate",
          filePath: "assets/asset-1/v001/v1.png",
          thumbnailPath: "thumbnails/asset-1/v1.webp",
          sha256: "abcdef1234",
          originalFilename: "first.png",
          mimeType: "image/png",
          byteSize: 2048,
          parentVersionId: null,
          createdAt: "2026-08-27T06:05:00Z",
        },
      ],
    });

    render(
      <AssetInspector projectRootPath="/projects/red-door" assetId="asset-1" />,
    );

    const version = await screen.findByTestId("asset-version");
    expect(version).toHaveTextContent("first.png");
    expect(version).toHaveTextContent("abcdef1234");
    expect(version).toHaveTextContent("2048");
    expect(version).toHaveTextContent("image/png");
    expect(version).toHaveTextContent("assets/asset-1/v001/v1.png");
    expect(screen.getByText("No canonical version")).toBeInTheDocument();

    const thumbnail = screen.getByRole("img") as HTMLImageElement;
    expect(thumbnail.src).toContain(
      "mock-asset://",
    );
  });
});
