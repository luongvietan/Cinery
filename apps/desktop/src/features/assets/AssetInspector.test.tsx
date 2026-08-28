import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AssetInspector } from "./AssetInspector";
import { getAssetWithVersions, promoteAssetVersion } from "./api";

vi.mock("./api");
vi.mock("./shell", () => ({
  openAssetFolder: vi.fn(),
  openProjectRelativePath: vi.fn(),
  revealProjectRelativePath: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `mock-asset://${path}`,
}));

const baseVersion = {
  assetId: "asset-1",
  filePath: "assets/asset-1/v002/v2.png",
  thumbnailPath: "thumbnails/asset-1/v2.webp",
  sha256: "hash2",
  originalFilename: "second.png",
  mimeType: "image/png" as const,
  byteSize: 100,
  width: 2048,
  height: 2048,
  parentVersionId: null,
  createdAt: "2026-08-27T06:10:00Z",
};

describe("AssetInspector", () => {
  beforeEach(() => {
    vi.mocked(getAssetWithVersions).mockReset();
    vi.mocked(promoteAssetVersion).mockReset();
    vi.restoreAllMocks();
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
          versionNumber: 2,
          status: "candidate",
          origin: "generated",
          generationArtifactId: "artifact-1",
          ...baseVersion,
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
          width: 1024,
          height: 1024,
          parentVersionId: null,
          createdAt: "2026-08-27T06:05:00Z",
        },
      ],
    });

    render(
      <AssetInspector projectRootPath="/projects/red-door" assetId="asset-1" />,
    );

    const versions = await screen.findAllByTestId("asset-version");
    expect(versions[0]).toHaveTextContent("v002");
    expect(versions[0]).toHaveTextContent("Candidate");
    expect(versions[1]).toHaveTextContent("v001");
    expect(versions[1]).toHaveTextContent("Canonical");
    expect(screen.getByText("Canonical: v001")).toBeInTheDocument();
    expect(versions[0]).toHaveTextContent("GENERATED");
    expect(versions[0]).toHaveTextContent("View generation details");
  });

  it("shows metadata and distinguishes no canonical from no versions", async () => {
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
          versionNumber: 1,
          status: "candidate",
          assetId: "asset-1",
          filePath: "assets/asset-1/v001/v1.png",
          thumbnailPath: "thumbnails/asset-1/v1.webp",
          sha256: "abcdef1234",
          originalFilename: "first.png",
          mimeType: "image/png",
          byteSize: 2048,
          width: 512,
          height: 512,
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
    expect(version).toHaveTextContent("512 × 512");
    expect(version).toHaveTextContent("PNG");
    expect(screen.getByText("Canonical: No canonical version")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Reveal" })).toBeInTheDocument();
  });

  it("promotes a candidate only after explicit user action and refreshes backend state", async () => {
    const initial = {
      asset: {
        id: "asset-1",
        projectId: "project-1",
        type: "face_lock" as const,
        label: "MARA-FACE",
        ownerEntityId: null,
        canonicalVersionId: "v1",
        createdAt: "2026-08-27T06:00:00Z",
        updatedAt: "2026-08-27T06:10:00Z",
      },
      versions: [
        {
          id: "v2",
          versionNumber: 2,
          status: "candidate" as const,
          ...baseVersion,
        },
        {
          id: "v1",
          assetId: "asset-1",
          versionNumber: 1,
          status: "canonical" as const,
          filePath: "assets/asset-1/v001/v1.png",
          thumbnailPath: "thumbnails/asset-1/v1.webp",
          sha256: "hash1",
          originalFilename: "first.png",
          mimeType: "image/png" as const,
          byteSize: 100,
          width: 1024,
          height: 1024,
          parentVersionId: null,
          createdAt: "2026-08-27T06:05:00Z",
        },
      ],
    };
    const refreshed = {
      asset: { ...initial.asset, canonicalVersionId: "v2" },
      versions: [
        { ...initial.versions[0], status: "canonical" as const },
        { ...initial.versions[1], status: "superseded" as const },
      ],
    };
    vi.mocked(getAssetWithVersions)
      .mockResolvedValueOnce(initial)
      .mockResolvedValueOnce(refreshed);
    vi.mocked(promoteAssetVersion).mockResolvedValue({
      asset: refreshed.asset,
      promotedVersion: refreshed.versions[0],
      supersededVersionId: "v1",
    });
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    const user = userEvent.setup();

    render(
      <AssetInspector projectRootPath="/projects/red-door" assetId="asset-1" />,
    );

    const promoteButton = await screen.findByRole("button", {
      name: "Set Canonical",
    });
    expect(promoteAssetVersion).not.toHaveBeenCalled();

    await user.click(promoteButton);

    expect(confirm).toHaveBeenCalledWith(
      "Make v002 the canonical version of MARA-FACE?\nThe current canonical version will be preserved and marked Superseded.",
    );
    expect(promoteAssetVersion).toHaveBeenCalledWith({
      projectRootPath: "/projects/red-door",
      assetVersionId: "v2",
    });
    expect(await screen.findByText("Canonical: v002")).toBeInTheDocument();
    expect(getAssetWithVersions).toHaveBeenCalledTimes(2);
  });
});
