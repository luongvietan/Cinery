import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  AssetVersion,
  AssetWithVersions,
  ProjectSummary,
} from "@cinematic/domain";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProjectWorkspace } from "./ProjectWorkspace";
import {
  getAssetWithVersions,
  importAssetVersion,
  listAssets,
} from "../assets/api";

vi.mock("../assets/api");
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

const project: ProjectSummary = {
  id: "project-1",
  name: "Red Door",
  rootPath: "/projects/red-door",
  schemaVersion: 1,
  createdAt: "2026-08-27T06:00:00Z",
  updatedAt: "2026-08-27T06:00:00Z",
};

function assetWithVersions(assetId: string, label: string): AssetWithVersions {
  return {
    asset: {
      id: assetId,
      projectId: "project-1",
      type: "face_lock",
      label,
      ownerEntityId: null,
      canonicalVersionId: null,
      createdAt: "2026-08-27T06:00:00Z",
      updatedAt: "2026-08-27T06:00:00Z",
    },
    versions: [],
  };
}

describe("ProjectWorkspace", () => {
  beforeEach(() => {
    vi.mocked(listAssets).mockReset().mockResolvedValue([
      {
        id: "asset-a",
        projectId: "project-1",
        type: "face_lock",
        label: "Asset A",
        ownerEntityId: null,
        canonicalVersionId: null,
        createdAt: "2026-08-27T06:00:00Z",
        updatedAt: "2026-08-27T06:00:00Z",
      },
      {
        id: "asset-b",
        projectId: "project-1",
        type: "outfit",
        label: "Asset B",
        ownerEntityId: null,
        canonicalVersionId: null,
        createdAt: "2026-08-27T06:00:00Z",
        updatedAt: "2026-08-27T06:00:00Z",
      },
    ]);
    vi.mocked(getAssetWithVersions).mockReset().mockImplementation(
      (_projectRootPath: string, assetId: string) =>
        Promise.resolve(
          assetWithVersions(assetId, assetId === "asset-a" ? "Asset A" : "Asset B"),
        ),
    );
    vi.mocked(importAssetVersion).mockReset();
    vi.mocked(open).mockReset().mockResolvedValue("/tmp/second.png");
  });

  it("resets the import button's in-flight state when switching assets mid-import", async () => {
    let resolveImport!: (value: AssetVersion) => void;
    let rejectImport!: (reason: unknown) => void;
    vi.mocked(importAssetVersion).mockReturnValue(
      new Promise<AssetVersion>((resolve, reject) => {
        resolveImport = resolve;
        rejectImport = reject;
      }),
    );

    const user = userEvent.setup();
    render(<ProjectWorkspace project={project} />);

    await user.click(screen.getByRole("button", { name: "Assets" }));
    await screen.findByText("Asset A");

    // Select asset A and start an import that never resolves during this test.
    await user.click(screen.getByRole("button", { name: /Asset A/ }));
    await screen.findByRole("heading", { level: 2, name: "Asset A" });
    await user.click(
      screen.getByRole("button", { name: "Import New Version" }),
    );

    const importButton = await screen.findByRole("button", {
      name: "Import New Version",
    });
    expect(importButton).toBeDisabled();

    // Switch to asset B before A's import settles.
    await user.click(screen.getByRole("button", { name: /Asset B/ }));
    await screen.findByRole("heading", { level: 2, name: "Asset B" });

    const importButtonForB = screen.getByRole("button", {
      name: "Import New Version",
    });
    expect(importButtonForB).not.toBeDisabled();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();

    // A's import now rejects - since the button remounted on the asset
    // switch, this must not surface as an error under asset B's button.
    rejectImport({ message: "disk is full" });
    await Promise.resolve();
    await Promise.resolve();

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Import New Version" }),
    ).not.toBeDisabled();

    // Keep the linter happy about the unused resolver - the promise already
    // settled via rejectImport above, so this is a harmless no-op.
    void resolveImport;
  });
});
