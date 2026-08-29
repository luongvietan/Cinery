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

describe("WorkflowWorkspace operation routing (regression)", () => {
  const qaOperation = {
    id: "asset.run_visual_qa",
    name: "Run Visual QA",
    description: "Evaluate one exact image asset version.",
    intentExamples: [],
    inputSchemaId: "run_visual_qa",
    prerequisites: [],
    tbdGuards: [],
    workflow: [{ id: "validate-input", type: "validate_input" } as never],
    expectedOutput: null,
  };

  it("does not open a character form for QA operations and never submits the wrong skill", async () => {
    const user = userEvent.setup();
    vi.mocked(listSkillOperations).mockResolvedValue([qaOperation]);
    render(<WorkflowWorkspace projectRootPath="C:/projects/red-door" />);

    const opButton = await screen.findByRole("button", { name: /Run Visual QA/ });
    await user.click(opButton);

    // No character form fields may appear; the user is pointed at the
    // operation's real entry point instead.
    expect(screen.queryByRole("combobox", { name: "Character" })).not.toBeInTheDocument();
    expect(await screen.findByRole("status")).toHaveTextContent(/Assets panel/);
    expect(screen.queryByRole("button", { name: "Execute" })).not.toBeInTheDocument();
  });

  it("keeps unknown operations from submitting any skill", async () => {
    const user = userEvent.setup();
    vi.mocked(listSkillOperations).mockResolvedValue([
      {
        ...qaOperation,
        id: "mystery.some_operation",
        name: "Mystery Operation",
      },
    ]);
    render(<WorkflowWorkspace projectRootPath="C:/projects/red-door" />);

    await user.click(await screen.findByRole("button", { name: /Mystery Operation/ }));
    expect(await screen.findByRole("status")).toHaveTextContent(/production context/);
    expect(screen.queryByRole("combobox", { name: "Character" })).not.toBeInTheDocument();
  });
});
