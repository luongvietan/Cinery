import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { AssetSummary, SkillOperation, WorkflowCharacterOption, WorkflowRunRecord } from "@cinematic/domain";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProductionWorkspace } from "./ProductionWorkspace";
import { listAssets } from "../assets/api";
import { listSkillOperations, listWorkflowCharacters, listWorkflowRuns } from "../workflows/api";

vi.mock("../assets/api");
vi.mock("../workflows/api");
vi.mock("./api");

const operation = {
  id: "character.create_face_lock",
  name: "Create Face Lock",
  description: "Create a consistent production reference for a character.",
} as SkillOperation;
const character = { id: "mara", name: "Mara" } as WorkflowCharacterOption;
const asset = {
  id: "face-asset",
  projectId: "project-1",
  type: "face_lock",
  label: "MARA-FACE",
  ownerEntityId: "mara",
  canonicalVersionId: "face-v002",
  versionCount: 2,
  canonicalVersionNumber: 2,
  previewThumbnailPath: "thumbnails/face-v002.webp",
  createdAt: "now",
  updatedAt: "now",
} as AssetSummary;

describe("ProductionWorkspace", () => {
  beforeEach(() => {
    vi.mocked(listSkillOperations).mockResolvedValue([operation]);
    vi.mocked(listWorkflowCharacters).mockResolvedValue([character]);
    vi.mocked(listWorkflowRuns).mockResolvedValue([] as WorkflowRunRecord[]);
    vi.mocked(listAssets).mockResolvedValue([asset]);
  });

  it("presents the golden face-lock operation with its canonical source version", async () => {
    render(<ProductionWorkspace projectRootPath="/projects/red-door" />);

    expect(await screen.findByRole("heading", { name: "Production" })).toBeInTheDocument();
    expect(screen.getByText("Create a consistent production reference for a character.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Create Face Lock" }));
    expect(await screen.findByRole("heading", { name: "Create Face Lock" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /MARA-FACE.*v002/ })).toBeInTheDocument();
  });
});
