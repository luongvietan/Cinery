import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { WorkflowWorkspace } from "./WorkflowWorkspace";
import { advanceWorkflowRun, listSkillOperations, listWorkflowCharacters, listWorkflowRuns } from "./api";

vi.mock("./api");

describe("WorkflowWorkspace (Generations)", () => {
  beforeEach(() => {
    vi.mocked(listSkillOperations).mockResolvedValue([
      {
        id: "character.create_face_lock",
        name: "Create Face Lock",
        description: "Compile a provider-neutral face-lock request.",
        intentExamples: [],
        inputSchemaId: "create_face_lock",
        prerequisites: [],
        tbdGuards: [],
        workflow: [{ id: "validate-input", type: "validate_input" } as never],
        expectedOutput: null,
      },
    ]);
    vi.mocked(listWorkflowRuns).mockResolvedValue([
      {
        id: "run-1",
        projectId: "project-1",
        skillId: "character-builder",
        skillVersion: "1.0.0",
        operationId: "character.create_face_lock",
        status: "waiting_for_approval",
        inputJson: "{}",
        prerequisiteReportJson: null,
        contextSnapshotJson: null,
        currentStepIndex: 3,
        failureCode: null,
        failureMessage: null,
        createdAt: "2026-08-28T00:00:00Z",
        updatedAt: "2026-08-28T00:00:00Z",
        completedAt: null,
      },
    ]);
    vi.mocked(listWorkflowCharacters).mockResolvedValue([{ id: "mara", name: "Mara" }]);
    vi.mocked(advanceWorkflowRun).mockReset();
  });

  it("shows history and tools without auto-advancing a run", async () => {
    render(<WorkflowWorkspace projectRootPath="C:/projects/red-door" />);

    expect(await screen.findByRole("heading", { name: "Generations" })).toBeInTheDocument();
    expect(await screen.findByText("Needs your approval")).toBeInTheDocument();
    expect(advanceWorkflowRun).not.toHaveBeenCalled();

    const launchButton = screen.getByRole("button", { name: "Generate face reference" });
    await userEvent.click(launchButton);
    await waitFor(() => expect(screen.getByRole("combobox", { name: "Character" })).toHaveValue("mara"));
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(launchButton).toHaveFocus());
  });

  it("keeps character runs human-named in history", async () => {
    render(<WorkflowWorkspace projectRootPath="C:/projects/red-door" />);
    expect(await screen.findByText("Face reference")).toBeInTheDocument();
    expect(screen.queryByText(/character-builder@1\.0\.0/)).not.toBeInTheDocument();
  });

  it("shows an empty state with direction when nothing has been generated", async () => {
    vi.mocked(listWorkflowRuns).mockResolvedValue([]);
    render(<WorkflowWorkspace projectRootPath="C:/projects/red-door" />);
    expect(await screen.findByText(/Nothing generated yet/i)).toBeInTheDocument();
  });
});

describe("WorkflowWorkspace operation routing (regression)", () => {
  it("never renders non-character operations as launchable tools", async () => {
    vi.mocked(listSkillOperations).mockResolvedValue([
      {
        id: "asset.run_visual_qa",
        name: "Run Visual QA",
        description: "Evaluate one exact image asset version.",
        intentExamples: [],
        inputSchemaId: "run_visual_qa",
        prerequisites: [],
        tbdGuards: [],
        workflow: [{ id: "validate-input", type: "validate_input" } as never],
        expectedOutput: null,
      } as never,
    ]);
    vi.mocked(listWorkflowRuns).mockResolvedValue([]);
    vi.mocked(listWorkflowCharacters).mockResolvedValue([]);
    render(<WorkflowWorkspace projectRootPath="C:/projects/red-door" />);

    // QA runs from the Assets panel; it must not appear as a tool here.
    expect(screen.queryByRole("button", { name: /Run Visual QA/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: "Character" })).not.toBeInTheDocument();
  });
});
