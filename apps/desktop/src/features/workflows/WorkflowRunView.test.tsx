import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { WorkflowRunDetail, WorkflowRunStatus } from "@cinematic/domain";
import { WorkflowRunView } from "./WorkflowRunView";
import { advanceWorkflowRun, approveWorkflowStep, rejectWorkflowStep } from "./api";

vi.mock("./api");

function detail(status: WorkflowRunStatus): WorkflowRunDetail {
  return {
    run: {
      id: "run-1", projectId: "project-1", skillId: "character-builder", skillVersion: "1.0.0",
      operationId: "character.create_face_lock", status, inputJson: "{}", prerequisiteReportJson: "{}",
      contextSnapshotJson: "{}", currentStepIndex: 3, failureCode: null, failureMessage: null,
      createdAt: "2026-08-28T00:00:00Z", updatedAt: "2026-08-28T00:00:00Z", completedAt: null,
    },
    steps: [
      {
        id: "step-1", workflowRunId: "run-1", stepDefinitionId: "approve-request", stepIndex: 3,
        stepType: "approval", status: status === "waiting_for_approval" ? "waiting" : "completed",
        inputJson: null, outputJson: null, startedAt: null, completedAt: null,
      },
    ],
    events: [],
  };
}

describe("WorkflowRunView", () => {
  it("records approval without executing the run", async () => {
    const waiting = detail("waiting_for_approval");
    const ready = detail("ready_for_execution");
    const onChange = vi.fn();
    vi.mocked(approveWorkflowStep).mockResolvedValue(ready);
    vi.mocked(advanceWorkflowRun).mockReset();

    render(<WorkflowRunView projectRootPath="C:/projects/red-door" detail={waiting} onChange={onChange} />);
    await userEvent.click(screen.getByRole("button", { name: "Approve request" }));

    expect(approveWorkflowStep).toHaveBeenCalledWith("C:/projects/red-door", "run-1", "approve-request", null);
    expect(advanceWorkflowRun).not.toHaveBeenCalled();
    expect(onChange).toHaveBeenCalledWith(ready);
  });

  it("exposes DryRun execution as a separate explicit action", async () => {
    const ready = detail("ready_for_execution");
    const completed = detail("completed");
    vi.mocked(advanceWorkflowRun).mockResolvedValue(completed);

    render(<WorkflowRunView projectRootPath="C:/projects/red-door" detail={ready} onChange={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: "Execute Dry Run" }));

    expect(advanceWorkflowRun).toHaveBeenCalledWith("C:/projects/red-door", "run-1");
  });

  it("requires an explicit confirmation before rejection", async () => {
    const waiting = detail("waiting_for_approval");
    vi.mocked(rejectWorkflowStep).mockResolvedValue(detail("rejected"));

    render(<WorkflowRunView projectRootPath="C:/projects/red-door" detail={waiting} onChange={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: "Reject request" }));
    expect(rejectWorkflowStep).not.toHaveBeenCalled();

    await userEvent.click(screen.getByRole("button", { name: "Confirm rejection" }));
    expect(rejectWorkflowStep).toHaveBeenCalledWith("C:/projects/red-door", "run-1", "approve-request", null);
  });
});
