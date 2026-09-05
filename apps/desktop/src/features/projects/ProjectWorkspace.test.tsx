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
import { listSkillOperations, listWorkflowRuns } from "../workflows/api";

vi.mock("../assets/api");
vi.mock("../workflows/api");
vi.mock("../assets/shell", () => ({
  openAssetFolder: vi.fn(),
  openProjectRelativePath: vi.fn(),
  revealProjectRelativePath: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `mock-asset://${path}`,
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
        versionCount: 0,
        canonicalVersionNumber: null,
        previewThumbnailPath: null,
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
        versionCount: 0,
        canonicalVersionNumber: null,
        previewThumbnailPath: null,
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
    vi.mocked(listSkillOperations).mockReset().mockResolvedValue([]);
    vi.mocked(listWorkflowRuns).mockReset().mockResolvedValue([]);
  });

  it("opens the project generations workspace from primary navigation", async () => {
    render(<ProjectWorkspace project={project} onCloseProject={vi.fn()} />);

    await userEvent.click(screen.getByRole("button", { name: "Generations" }));

    expect(await screen.findByRole("heading", { name: /Generation tools/i })).toBeInTheDocument();
    expect(listWorkflowRuns).toHaveBeenCalledWith(project.rootPath);
  });

  it("labels the stable scenes panel as Sequences in project navigation", async () => {
    render(<ProjectWorkspace project={project} onCloseProject={vi.fn()} />);

    await userEvent.click(screen.getByRole("button", { name: "Sequences" }));

    expect(await screen.findByRole("region", { name: /Scenes workspace/i })).toBeInTheDocument();
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
    render(
      <ProjectWorkspace project={project} onCloseProject={vi.fn()} />,
    );

    await user.click(screen.getByRole("button", { name: "Assets" }));
    await screen.findByText("Asset A");

    await user.click(screen.getByRole("button", { name: /Asset A/ }));
    await screen.findByRole("heading", { level: 2, name: "Asset A" });
    await user.click(screen.getByRole("button", { name: "Import Version" }));

    const importButton = await screen.findByRole("button", {
      name: "Importing…",
    });
    expect(importButton).toBeDisabled();

    await user.click(screen.getByRole("button", { name: /Asset B/ }));
    await screen.findByRole("heading", { level: 2, name: "Asset B" });

    const importButtonForB = screen.getByRole("button", {
      name: "Import Version",
    });
    expect(importButtonForB).not.toBeDisabled();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();

    rejectImport({ message: "disk is full" });
    await Promise.resolve();
    await Promise.resolve();

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Import Version" }),
    ).not.toBeDisabled();

    void resolveImport;
  });

  it("navigates back through workspace, assets list, and asset detail", async () => {
    const onCloseProject = vi.fn();
    const user = userEvent.setup();
    render(
      <ProjectWorkspace project={project} onCloseProject={onCloseProject} />,
    );

    await user.click(screen.getByRole("button", { name: "Assets" }));
    expect(screen.getByRole("button", { name: "← Workspace" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Asset A/ }));
    expect(
      await screen.findByRole("heading", { level: 2, name: "Asset A" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "← Assets" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "← Assets" }));
    expect(
      screen.queryByRole("heading", { level: 2, name: "Asset A" }),
    ).not.toBeInTheDocument();
    expect(screen.getByText("Asset A")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "← Workspace" }));
    expect(screen.queryByText("Asset A")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "← Projects" }));
    expect(onCloseProject).toHaveBeenCalledTimes(1);
  });
});
