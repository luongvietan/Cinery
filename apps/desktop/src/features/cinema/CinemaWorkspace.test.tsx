import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CinemaWorkspace } from "./CinemaWorkspace";
import { addSceneCharacter, compileCinema, createScene, createShot, getScene, listScenes } from "./api";
import { listAssets } from "../assets/api";
import { listCanonEntities } from "../canon/api";

vi.mock("./api", () => ({ addSceneCharacter: vi.fn(), compileCinema: vi.fn(), createScene: vi.fn(), createShot: vi.fn(), getScene: vi.fn(), listScenes: vi.fn() }));
vi.mock("../assets/api", () => ({ listAssets: vi.fn() }));
vi.mock("../canon/api", () => ({ listCanonEntities: vi.fn() }));

const root = "/projects/red-door";

describe("CinemaWorkspace", () => {
  beforeEach(() => {
    vi.mocked(listAssets).mockReset().mockResolvedValue([
      { id: "world", projectId: "project-1", type: "world_plate", label: "Station", ownerEntityId: null, canonicalVersionId: "world-v1", versionCount: 1, canonicalVersionNumber: 1, previewThumbnailPath: null, createdAt: "now", updatedAt: "now" },
      { id: "look", projectId: "project-1", type: "outfit", label: "Mara Look", ownerEntityId: "mara", canonicalVersionId: "look-v1", versionCount: 1, canonicalVersionNumber: 1, previewThumbnailPath: null, createdAt: "now", updatedAt: "now" },
      { id: "sheet", projectId: "project-1", type: "character_sheet", label: "Mara Sheet", ownerEntityId: "mara", canonicalVersionId: "sheet-v1", versionCount: 1, canonicalVersionNumber: 1, previewThumbnailPath: null, createdAt: "now", updatedAt: "now" },
    ]);
    vi.mocked(listCanonEntities).mockReset().mockResolvedValue([{ id: "mara", projectId: "project-1", type: "character", name: "Mara", slug: "mara", createdAt: "now", updatedAt: "now" }]);
    vi.mocked(listScenes).mockReset().mockResolvedValue([]);
    vi.mocked(createScene).mockReset().mockResolvedValue({ id: "scene-001", projectId: "project-1", title: "Scene 001", worldAssetVersionId: "world-v1", canonNotes: null, createdAt: "now", updatedAt: "now" });
    vi.mocked(addSceneCharacter).mockReset().mockResolvedValue({ scene: { id: "scene-001" }, characters: [], props: [], shots: [] } as never);
    vi.mocked(createShot).mockReset().mockResolvedValue({ id: "shot-001" } as never);
    vi.mocked(getScene).mockReset();
    vi.mocked(compileCinema).mockReset();
  });

  it("stages a scene with existing canonical records through the real cinema command sequence", async () => {
    render(<CinemaWorkspace projectRootPath={root} action={{ id: "scene", title: "Scene", destination: "cinema", characterEntityId: "mara", sceneId: null }} />);

    await userEvent.click(await screen.findByRole("button", { name: "Stage Scene" }));

    expect(await screen.findByText("Scene 001 staged")).toBeInTheDocument();
    expect(createScene).toHaveBeenCalledWith(root, "Scene 001", "world-v1");
    expect(addSceneCharacter).toHaveBeenCalledWith(root, "scene-001", "mara", "look-v1", "sheet-v1");
    expect(createShot).toHaveBeenCalledWith(root, "scene-001", "Establish the scene");
  });

  it("compiles the scoped ready scene using its persisted shot duration", async () => {
    vi.mocked(listScenes).mockResolvedValue([{ id: "scene-002", projectId: "project-1", title: "Scene 002", worldAssetVersionId: "world-v1", canonNotes: null, createdAt: "now", updatedAt: "now" }]);
    vi.mocked(getScene).mockResolvedValue({ scene: { id: "scene-002" }, characters: [], props: [], shots: [{ durationSeconds: 4 }] } as never);
    vi.mocked(compileCinema).mockResolvedValue({ id: "comp-001" } as never);

    render(<CinemaWorkspace projectRootPath={root} action={{ id: "cinema_compilation", title: "Cinema Compilation", destination: "cinema", characterEntityId: null, sceneId: "scene-002" }} />);

    await userEvent.click(await screen.findByRole("button", { name: "Compile Scene 002" }));

    expect(await screen.findByText("Cinema prompt compiled")).toBeInTheDocument();
    expect(compileCinema).toHaveBeenCalledWith(root, "scene-002", 4);
  });
});
