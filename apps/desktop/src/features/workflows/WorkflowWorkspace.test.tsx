import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { WorkflowWorkspace } from "./WorkflowWorkspace";
import { advanceWorkflowRun, listSkillOperations, listWorkflowCharacters, listWorkflowRuns } from "./api";

vi.mock("./api");

describe("WorkflowWorkspace", () => {
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
        workflow: [{ id: "validate-input", type: "validate_input" }],
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

  it("shows operations and persisted state without auto-advancing a run", async () => {
    render(<WorkflowWorkspace projectRootPath="C:/projects/red-door" />);

    expect(await screen.findByRole("heading", { name: "Create Face Lock" })).toBeInTheDocument();
    expect(await screen.findByText("Waiting for approval")).toBeInTheDocument();
    expect(advanceWorkflowRun).not.toHaveBeenCalled();

    const launchButton = screen.getByRole("button", { name: "Create Face Lock" });
    await userEvent.click(launchButton);
    await waitFor(() => expect(screen.getByRole("combobox", { name: "Character" })).toHaveValue("mara"));
    expect(screen.getByRole("combobox", { name: "Character" })).toHaveFocus();
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(launchButton).toHaveFocus());
  });
});
