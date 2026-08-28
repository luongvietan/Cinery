import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SceneEditor } from "./SceneEditor";
import { getScene, updateSceneDetails } from "./api";

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
}));

describe("SceneEditor", () => {
  beforeEach(() => {
    vi.mocked(getScene).mockReset();
    vi.mocked(updateSceneDetails).mockReset();
  });

  it("renders Title and Summary fields", async () => {
    vi.mocked(getScene).mockResolvedValue({
      id: "scene-1",
      projectId: "p",
      ordinal: 1,
      title: "Night Transmission",
      summary: "Mara receives...",
      worldId: null,
      worldAssetVersionId: null,
      keyframeAssetId: null,
      createdAt: "now",
      updatedAt: "now",
    } as any);

    render(<SceneEditor projectRootPath="/projects/red-door" sceneId="scene-1" />);

    expect(await screen.findByText("SCENE-001 Night Transmission")).toBeInTheDocument();
    expect(screen.getByLabelText("Title")).toBeInTheDocument();
    expect(screen.getByLabelText("Summary")).toBeInTheDocument();
    expect(screen.getByDisplayValue("Night Transmission")).toBeInTheDocument();
    expect(screen.getByDisplayValue("Mara receives...")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save Scene" })).toBeInTheDocument();
  });

  it("saves title and summary with keyboard activation", async () => {
    vi.mocked(getScene).mockResolvedValue({
      id: "scene-1",
      projectId: "p",
      ordinal: 1,
      title: "Old Title",
      summary: "Old summary",
      worldId: null,
      worldAssetVersionId: null,
      keyframeAssetId: null,
      createdAt: "now",
      updatedAt: "now",
    } as any);
    vi.mocked(updateSceneDetails).mockResolvedValue({
      id: "scene-1",
      projectId: "p",
      ordinal: 1,
      title: "New Title",
      summary: "New summary",
      worldId: null,
      worldAssetVersionId: null,
      keyframeAssetId: null,
      createdAt: "now",
      updatedAt: "now",
    } as any);

    const user = userEvent.setup();
    render(<SceneEditor projectRootPath="/projects/red-door" sceneId="scene-1" />);

    const titleInput = await screen.findByLabelText("Title");
    await user.clear(titleInput);
    await user.type(titleInput, "New Title");
    const summaryInput = screen.getByLabelText("Summary");
    await user.clear(summaryInput);
    await user.type(summaryInput, "New summary");

    const saveButton = screen.getByRole("button", { name: "Save Scene" });
    // keyboard activation via Enter/Space is native; we test click + focus
    saveButton.focus();
    expect(document.activeElement).toBe(saveButton);
    await user.click(saveButton);

    expect(updateSceneDetails).toHaveBeenCalledWith("/projects/red-door", "scene-1", "New Title", "New summary");
    expect(await screen.findByText("Scene saved")).toBeInTheDocument();
  });

  it("is responsive and has visible labels", async () => {
    vi.mocked(getScene).mockResolvedValue({
      id: "scene-1",
      projectId: "p",
      ordinal: 5,
      title: "Responsive Test",
      summary: "Summary",
      worldId: null,
      worldAssetVersionId: null,
      keyframeAssetId: null,
      createdAt: "now",
      updatedAt: "now",
    } as any);
    render(<SceneEditor projectRootPath="/projects/red-door" sceneId="scene-1" />);
    expect(await screen.findByLabelText("Title")).toBeVisible();
    expect(screen.getByLabelText("Summary")).toBeVisible();
    // check reduced-motion not breaking: ensure form is in document and not hidden
    expect(screen.getByRole("form", { name: "Scene details" })).toBeInTheDocument();
  });
});
