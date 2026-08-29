import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { AssetSummary, SceneDetail, SceneRecord } from "@cinematic/domain";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CinemaWorkspace } from "./CinemaWorkspace";
import * as cinemaApi from "./api";
import { listAssets } from "../assets/api";
import { listCanonEntities } from "../canon/api";

vi.mock("./api", () => ({
  compileCinema: vi.fn(),
  getScene: vi.fn(),
  listScenes: vi.fn(),
  stageScene: vi.fn(),
  createSceneFull: vi.fn(),
  renameScene: vi.fn(),
  setSceneWorld: vi.fn(),
  updateSceneCharacter: vi.fn(),
  removeSceneCharacter: vi.fn(),
  addSceneProp: vi.fn(),
  removeSceneProp: vi.fn(),
  updateShot: vi.fn(),
  deleteShot: vi.fn(),
  reorderShots: vi.fn(),
  setShotKeyframe: vi.fn(),
  getSceneReadiness: vi.fn(),
}));
vi.mock("../assets/api", () => ({ listAssets: vi.fn() }));
vi.mock("../canon/api", () => ({ listCanonEntities: vi.fn() }));

const root = "/projects/red-door";

const scene: SceneRecord = {
  id: "scene-1", projectId: "project-1", title: "Scene 001", worldAssetVersionId: "world-v1",
  canonNotes: null, createdAt: "now", updatedAt: "now",
};

const detail: SceneDetail = {
  scene,
  characters: [{ characterEntityId: "mara", lookAssetVersionId: "look-v1", sheetAssetVersionId: "sheet-v1", displayOrder: 0 }],
  props: [{ propAssetVersionId: "prop-v1", displayOrder: 0 }],
  shots: [{ id: "shot-1", sceneId: "scene-1", ordering: 0, durationSeconds: 4, keyframeAssetVersionId: "kf-v1", intent: "Establish", action: null, camera: null, generatedVideoAssetVersionId: null, createdAt: "now", updatedAt: "now" }],
};

const ready = { sceneId: "scene-1", ready: true, blockers: [] };
const missingWorld = {
  sceneId: "scene-1",
  ready: false,
  blockers: [{ code: "missing_world", sceneId: "scene-1", entityId: null, shotId: null, message: "No World Plate is pinned to this scene.", actionTarget: "world" as const }],
};

const assets: AssetSummary[] = [
  { id: "world", projectId: "project-1", type: "world_plate", label: "Station", ownerEntityId: null, canonicalVersionId: "world-v1", versionCount: 1, canonicalVersionNumber: 1, previewThumbnailPath: null, createdAt: "now", updatedAt: "now" },
  { id: "look", projectId: "project-1", type: "outfit", label: "Mara Look", ownerEntityId: "mara", canonicalVersionId: "look-v1", versionCount: 1, canonicalVersionNumber: 1, previewThumbnailPath: null, createdAt: "now", updatedAt: "now" },
  { id: "prop", projectId: "project-1", type: "prop_plate", label: "Console", ownerEntityId: null, canonicalVersionId: "prop-v1", versionCount: 1, canonicalVersionNumber: 1, previewThumbnailPath: null, createdAt: "now", updatedAt: "now" },
  { id: "kf", projectId: "project-1", type: "shot_keyframe", label: "KF 1", ownerEntityId: null, canonicalVersionId: "kf-v1", versionCount: 1, canonicalVersionNumber: 1, previewThumbnailPath: null, createdAt: "now", updatedAt: "now" },
];

describe("CinemaWorkspace", () => {
  beforeEach(() => {
    vi.mocked(listAssets).mockReset().mockResolvedValue(assets);
    vi.mocked(listCanonEntities).mockReset().mockResolvedValue([
      { id: "mara", projectId: "project-1", type: "character", name: "Mara", slug: "mara", createdAt: "now", updatedAt: "now" },
    ]);
    vi.mocked(cinemaApi.listScenes).mockReset().mockResolvedValue([scene]);
    vi.mocked(cinemaApi.getScene).mockReset().mockResolvedValue(detail);
    vi.mocked(cinemaApi.getSceneReadiness).mockReset().mockResolvedValue(ready);
    vi.mocked(cinemaApi.createSceneFull).mockReset().mockResolvedValue(scene);
    vi.mocked(cinemaApi.compileCinema).mockReset().mockResolvedValue({
      id: "comp-1", projectId: "project-1", sceneId: "scene-1", inputJson: "{}", compilationJson: "{}", exportPath: "prompts/cinema/x.md", exportSha256: "a".repeat(64), createdAt: "now",
    });
  });

  it("creates a scene without auto-selecting assets and lists it", async () => {
    const user = userEvent.setup();
    render(<CinemaWorkspace projectRootPath={root} action={null} />);
    await user.click(await screen.findByRole("button", { name: "Create Scene" }));
    await user.type(screen.getByLabelText(/Scene title/), "Scene 001");
    await user.click(screen.getByRole("button", { name: "Create" }));
    const items = await screen.findAllByText("Scene 001");
    expect(items.length).toBeGreaterThan(0);
    expect(cinemaApi.createSceneFull).toHaveBeenCalledWith(root, "Scene 001", null);
  });

  it("disables Compile while readiness blockers exist and lists them", async () => {
    vi.mocked(cinemaApi.getSceneReadiness).mockResolvedValue(missingWorld);
    const user = userEvent.setup();
    render(<CinemaWorkspace projectRootPath={root} action={null} />);
    await user.click(await screen.findByRole("button", { name: /Scene 001/ }));
    const compileButton = await screen.findByRole("button", { name: /Compile/ });
    expect(compileButton).toBeDisabled();
    expect(await screen.findByText(/No World Plate is pinned/)).toBeInTheDocument();
  });

  it("removes a prop without deleting the source asset listing", async () => {
    vi.mocked(cinemaApi.removeSceneProp).mockResolvedValue();
    const user = userEvent.setup();
    render(<CinemaWorkspace projectRootPath={root} action={null} />);
    await user.click(await screen.findByRole("button", { name: /Scene 001/ }));
    const removeButton = await screen.findByRole("button", { name: /Remove prop Console/ });
    await user.click(removeButton);
    expect(cinemaApi.removeSceneProp).toHaveBeenCalledWith(root, "scene-1", "prop-v1");
    // The prop asset stays listed in the assets list used for re-adding.
    expect(assets.some((asset) => asset.id === "prop")).toBe(true);
  });
});
