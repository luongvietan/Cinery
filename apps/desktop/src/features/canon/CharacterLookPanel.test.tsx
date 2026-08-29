import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { AssetSummary } from "@cinematic/domain";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CharacterLookPanel } from "./CharacterLookPanel";
import { listAssets } from "../assets/api";

vi.mock("../assets/api");
vi.mock("../workflows/api");
vi.mock("../generation/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `mock-asset://${path}`,
}));

const character = { id: "mara", name: "Mara" };

function assetSummary(overrides: Partial<AssetSummary> = {}): AssetSummary {
  return {
    id: "asset-1",
    projectId: "project-1",
    type: "face_lock",
    label: "Mara Face",
    ownerEntityId: "mara",
    canonicalVersionId: "v1",
    versionCount: 1,
    canonicalVersionNumber: 1,
    previewThumbnailPath: null,
    createdAt: "now",
    updatedAt: "now",
    ...overrides,
  };
}

describe("CharacterLookPanel", () => {
  beforeEach(() => {
    vi.mocked(listAssets).mockReset().mockResolvedValue([]);
  });

  it("marks the face step approved when the character owns an approved face", async () => {
    vi.mocked(listAssets).mockResolvedValue([assetSummary()]);
    render(<CharacterLookPanel projectRootPath="C:/p" character={character} />);
    expect(await screen.findByText("Approved", { exact: false })).toBeInTheDocument();
    expect(screen.getByText("Generate another")).toBeInTheDocument();
  });

  it("blocks the outfit step until a face is approved and states why", async () => {
    vi.mocked(listAssets).mockResolvedValue([]);
    render(<CharacterLookPanel projectRootPath="C:/p" character={character} />);
    const outfitCard = await screen.findByText("Outfit");
    expect(outfitCard).toBeInTheDocument();
    expect(screen.getAllByText(/Needs an approved face first/).length).toBeGreaterThan(0);
  });

  it("opens the face form from the face card without leaving the character", async () => {
    const user = userEvent.setup();
    vi.mocked(listAssets).mockResolvedValue([]);
    render(<CharacterLookPanel projectRootPath="C:/p" character={character} />);
    await user.click(await screen.findByRole("button", { name: /Generate face reference/i }));
    expect(await screen.findByRole("heading", { name: "Generate face reference" })).toBeInTheDocument();
  });
});
