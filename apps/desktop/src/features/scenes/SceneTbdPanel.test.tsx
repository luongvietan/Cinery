import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SceneTbdPanel } from "./SceneTbdPanel";
import {
  getScene,
  listSceneCharacters,
  listSceneTbdBindings,
  setSceneTbdBinding,
} from "./api";
import { listCanonTbds } from "../canon/api";
import { listWorlds } from "../worlds/api";

vi.mock("./api", () => ({
  listScenes: vi.fn(),
  getScene: vi.fn(),
  createScene: vi.fn(),
  updateSceneDetails: vi.fn(),
  assignSceneWorld: vi.fn(),
  clearSceneWorld: vi.fn(),
  addSceneCharacter: vi.fn(),
  removeSceneCharacter: vi.fn(),
  listSceneCharacters: vi.fn(),
  addSceneProp: vi.fn(),
  removeSceneProp: vi.fn(),
  listSceneProps: vi.fn(),
  resolveSceneReferences: vi.fn(),
  upgradeSceneWorldReference: vi.fn(),
  upgradeSceneCharacterLookReference: vi.fn(),
  upgradeSceneCharacterSheetReference: vi.fn(),
  upgradeScenePropReference: vi.fn(),
  setSceneTbdBinding: vi.fn(),
  removeSceneTbdBinding: vi.fn(),
  listSceneTbdBindings: vi.fn(),
  setShotKeyframe: vi.fn(),
  setShotVideo: vi.fn(),
  getCompileReadiness: vi.fn(),
  compileCinema: vi.fn(),
  listCinemaCompilations: vi.fn(),
  listShots: vi.fn(),
  createShot: vi.fn(),
  updateShot: vi.fn(),
  deleteShot: vi.fn(),
  reorderShots: vi.fn(),
  ensureSceneKeyframeAsset: vi.fn(),
}));
vi.mock("../canon/api", () => ({
  listCanonTbds: vi.fn(),
  listCanonEntities: vi.fn(),
  getCanonEntity: vi.fn(),
}));
vi.mock("../worlds/api", () => ({
  listWorlds: vi.fn(),
  listWorldsDetailed: vi.fn(),
  getWorldDetailed: vi.fn(),
}));
vi.mock("../assets/api", () => ({
  listAssets: vi.fn(),
  getAssetWithVersions: vi.fn(),
}));

describe("SceneTbdPanel", () => {
  beforeEach(() => {
    vi.mocked(getScene).mockReset();
    vi.mocked(listSceneCharacters).mockReset();
    vi.mocked(listCanonTbds).mockReset();
    vi.mocked(listWorlds).mockReset();
    vi.mocked(listSceneTbdBindings).mockReset().mockResolvedValue([]);
    vi.mocked(setSceneTbdBinding).mockReset().mockResolvedValue({
      id: "binding-1",
      sceneId: "scene-1",
      canonTbdId: "tbd-1",
      topicSnapshot: "Location secret",
      noteSnapshot: null,
      decision: "preserve_unknown",
      justification: null,
      createdAt: "now",
      updatedAt: "now",
    });
  });

  it("shows all relevant TBD decisions", async () => {
    vi.mocked(getScene).mockResolvedValue({
      id: "scene-1",
      projectId: "p",
      ordinal: 1,
      title: "Scene",
      summary: "Summary",
      worldId: "world-1",
      worldAssetVersionId: "v1",
      keyframeAssetId: null,
      createdAt: "now",
      updatedAt: "now",
    } as any);
    vi.mocked(listWorlds).mockResolvedValue([
      { id: "world-1", projectId: "p", canonLocationEntityId: "loc-1", worldPlateAssetId: "asset-1", createdAt: "now", updatedAt: "now" } as any,
    ]);
    vi.mocked(listSceneCharacters).mockResolvedValue([
      { id: "assign-1", sceneId: "scene-1", characterEntityId: "char-1", lookAssetVersionId: "v1", sheetAssetVersionId: null, notes: null, createdAt: "now", updatedAt: "now" } as any,
    ]);
    vi.mocked(listCanonTbds).mockResolvedValue([
      { id: "tbd-1", projectId: "p", canonEntityId: "loc-1", sectionKey: null, topic: "Location secret", note: "Do not reveal", protected: true, status: "open", resolutionText: null, createdAt: "now", updatedAt: "now", resolvedAt: null },
      { id: "tbd-2", projectId: "p", canonEntityId: null, sectionKey: null, topic: "Global unknown", note: "Project note", protected: true, status: "open", resolutionText: null, createdAt: "now", updatedAt: "now", resolvedAt: null },
      { id: "tbd-3", projectId: "p", canonEntityId: "char-1", sectionKey: null, topic: "Character secret", note: null, protected: true, status: "open", resolutionText: null, createdAt: "now", updatedAt: "now", resolvedAt: null },
      // irrelevant TBD should be filtered out
      { id: "tbd-4", projectId: "p", canonEntityId: "loc-999", sectionKey: null, topic: "Irrelevant", note: null, protected: true, status: "open", resolutionText: null, createdAt: "now", updatedAt: "now", resolvedAt: null },
    ] as any);

    render(<SceneTbdPanel projectRootPath="/projects/red-door" sceneId="scene-1" />);

    expect(await screen.findByText("Location secret")).toBeInTheDocument();
    expect(screen.getByText("Global unknown")).toBeInTheDocument();
    expect(screen.getByText("Character secret")).toBeInTheDocument();
    expect(screen.queryByText("Irrelevant")).not.toBeInTheDocument();
    expect(screen.getAllByText("PROTECTED").length).toBe(3);
  });

  it("requires justification for not_applicable on project-scoped and validates directly scoped must be preserve_unknown", async () => {
    vi.mocked(getScene).mockResolvedValue({
      id: "scene-1",
      projectId: "p",
      ordinal: 1,
      title: "Scene",
      summary: "Summary",
      worldId: null,
      worldAssetVersionId: null,
      keyframeAssetId: null,
      createdAt: "now",
      updatedAt: "now",
    } as any);
    vi.mocked(listWorlds).mockResolvedValue([]);
    vi.mocked(listSceneCharacters).mockResolvedValue([]);
    vi.mocked(listCanonTbds).mockResolvedValue([
      { id: "tbd-proj", projectId: "p", canonEntityId: null, sectionKey: null, topic: "Global TBD", note: "Note", protected: true, status: "open", resolutionText: null, createdAt: "now", updatedAt: "now", resolvedAt: null },
      { id: "tbd-loc", projectId: "p", canonEntityId: "loc-1", sectionKey: null, topic: "Location TBD", note: null, protected: true, status: "open", resolutionText: null, createdAt: "now", updatedAt: "now", resolvedAt: null },
    ] as any);

    // need loc-1 relevant? Without world, no loc, so only project-scoped visible. Add world to make loc relevant
    vi.mocked(getScene).mockResolvedValue({
      id: "scene-1",
      projectId: "p",
      ordinal: 1,
      title: "Scene",
      summary: "Summary",
      worldId: "world-1",
      worldAssetVersionId: "v1",
      keyframeAssetId: null,
      createdAt: "now",
      updatedAt: "now",
    } as any);
    vi.mocked(listWorlds).mockResolvedValue([
      { id: "world-1", projectId: "p", canonLocationEntityId: "loc-1", worldPlateAssetId: "asset-1", createdAt: "now", updatedAt: "now" } as any,
    ]);

    const user = userEvent.setup();
    render(<SceneTbdPanel projectRootPath="/projects/red-door" sceneId="scene-1" />);

    expect(await screen.findByText("Global TBD")).toBeInTheDocument();
    expect(screen.getByText("Location TBD")).toBeInTheDocument();

    // Project-scoped: select not_applicable without justification should show error
    const notApplicableRadioForProj = screen.getByLabelText("Not applicable for Global TBD");
    await user.click(notApplicableRadioForProj);
    // justification required UI should appear
    expect(await screen.findByLabelText("Justification for Global TBD")).toBeInTheDocument();
    expect(screen.getByText("Justification is required for not_applicable")).toBeInTheDocument();

    // Provide justification
    const justificationInput = screen.getByLabelText("Justification for Global TBD");
    await user.type(justificationInput, "Not relevant to this exterior scene");
    expect(justificationInput).toHaveValue("Not relevant to this exterior scene");
    // error should disappear
    expect(screen.queryByText("Justification is required for not_applicable")).not.toBeInTheDocument();

    // Location-scoped: not_applicable should be disabled or show validation error
    const notApplicableForLoc = screen.getByLabelText("Not applicable for Location TBD");
    expect(notApplicableForLoc).toBeDisabled();
    // Try to click preserve_unknown for location
    const preserveForLoc = screen.getByLabelText("Preserve unknown for Location TBD");
    await user.click(preserveForLoc);
    expect(preserveForLoc).toBeChecked();
  });

  it("handles keyboard activation for radio buttons", async () => {
    vi.mocked(getScene).mockResolvedValue({
      id: "scene-1",
      projectId: "p",
      ordinal: 1,
      title: "Scene",
      summary: "Summary",
      worldId: "world-1",
      worldAssetVersionId: "v1",
      keyframeAssetId: null,
      createdAt: "now",
      updatedAt: "now",
    } as any);
    vi.mocked(listWorlds).mockResolvedValue([
      { id: "world-1", projectId: "p", canonLocationEntityId: "loc-1", worldPlateAssetId: "asset-1", createdAt: "now", updatedAt: "now" } as any,
    ]);
    vi.mocked(listSceneCharacters).mockResolvedValue([]);
    vi.mocked(listCanonTbds).mockResolvedValue([
      { id: "tbd-1", projectId: "p", canonEntityId: "loc-1", sectionKey: null, topic: "Keyboard TBD", note: null, protected: true, status: "open", resolutionText: null, createdAt: "now", updatedAt: "now", resolvedAt: null },
    ] as any);

    const user = userEvent.setup();
    render(<SceneTbdPanel projectRootPath="/projects/red-door" sceneId="scene-1" />);

    const preserveRadio = await screen.findByLabelText("Preserve unknown for Keyboard TBD");
    preserveRadio.focus();
    expect(document.activeElement).toBe(preserveRadio);
    await user.keyboard(" ");
    expect(preserveRadio).toBeChecked();

    // The decision persists through the command boundary (P10.0 fix: the
    // panel previously kept decisions in local state only).
    await vi.waitFor(() =>
      expect(setSceneTbdBinding).toHaveBeenCalledWith(
        "/projects/red-door",
        "scene-1",
        "tbd-1",
        "preserve_unknown",
        null,
      ),
    );
  });
});
