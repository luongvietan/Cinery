import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi, beforeEach } from "vitest";
import type { WorkflowRunDetail, WorkflowRunStatus } from "@cinematic/domain";
import { WorkflowRunView } from "./WorkflowRunView";
import {
  advanceWorkflowRun,
  approveWorkflowStep,
  cancelWorkflowExecution,
  getWorkflowRun,
} from "./api";

vi.mock("./api");

function detail(status: WorkflowRunStatus, overrides: Partial<WorkflowRunDetail["run"]> = {}): WorkflowRunDetail {
  return {
    run: {
      id: "run-1", projectId: "project-1", skillId: "scene-builder", skillVersion: "1.0.0",
      operationId: "scene.generate_video", status, inputJson: JSON.stringify({ providerId: "fake_async_video", modelId: "fake-video-v1", sceneId: "scene-1" }), prerequisiteReportJson: "{}",
      contextSnapshotJson: "{}", currentStepIndex: 4, failureCode: null, failureMessage: null,
      createdAt: "2026-08-30T00:00:00Z", updatedAt: "2026-08-30T00:00:00Z", completedAt: null,
      ...overrides,
    },
    steps: [
      {
        id: "step-exec", workflowRunId: "run-1", stepDefinitionId: "execute", stepIndex: 4,
        stepType: "execute", status: "running",
        inputJson: null, outputJson: null, startedAt: null, completedAt: null,
      },
    ],
    events: [],
    providerExecutions: [
      {
        id: "attempt-1", stepDefinitionId: "execute", attemptNumber: 1,
        providerId: "fake_async_video", modelId: "fake-video-v1", adapterVersion: 1,
        status: "running", providerJobId: "fake-job-1", normalizedErrorJson: null,
        startedAt: "2026-08-30T00:00:01Z", completedAt: null,
      },
    ],
  };
}

describe("WorkflowRunView — background execution (P10.1)", () => {
  beforeEach(() => {
    vi.mocked(getWorkflowRun).mockReset();
    vi.mocked(advanceWorkflowRun).mockReset();
    vi.mocked(cancelWorkflowExecution).mockReset();
    vi.mocked(approveWorkflowStep).mockReset();
  });

  it("shows the provider and model while a background job runs, with a cancel action that returns immediately", async () => {
    vi.mocked(cancelWorkflowExecution).mockResolvedValue(detail("cancelled"));
    render(
      <WorkflowRunView
        projectRootPath="C:/projects/red-door"
        detail={detail("running")}
        onChange={vi.fn()}
      />,
    );

    expect(screen.getAllByText(/fake_async_video/).length).toBeGreaterThan(0);
    expect(screen.getByRole("heading", { name: /Generating/ })).toBeInTheDocument();
    expect(screen.getByText(/You can leave this page/)).toBeInTheDocument();
    expect(screen.getAllByText(/Attempt 1/).length).toBeGreaterThan(0);

    // Cancel is a separate command; the user never waits on the original
    // generation invoke.
    await userEvent.click(screen.getAllByRole("button", { name: "Cancel generation" })[0]);
    expect(cancelWorkflowExecution).toHaveBeenCalledWith("C:/projects/red-door", "run-1", "execute");
  });

  it("refreshes authoritative state while non-terminal and surfaces the runner's completion", async () => {
    const running = detail("running");
    const completed = detail("completed", {
      completedAt: "2026-08-30T00:01:00Z",
      currentStepIndex: 6,
    });
    completed.steps[0].status = "completed";
    completed.providerExecutions = [];
    vi.mocked(getWorkflowRun).mockResolvedValue(completed);
    const onChange = vi.fn();

    render(
      <WorkflowRunView
        projectRootPath="C:/projects/red-door"
        detail={running}
        onChange={onChange}
      />,
    );

    await waitFor(
      () => expect(onChange).toHaveBeenCalledWith(completed),
      { timeout: 4000 },
    );
    expect(getWorkflowRun).toHaveBeenCalledWith("C:/projects/red-door", "run-1");
  });

  it("does not refresh a terminal run", async () => {
    vi.mocked(getWorkflowRun).mockClear();
    render(
      <WorkflowRunView
        projectRootPath="C:/projects/red-door"
        detail={detail("completed")}
        onChange={vi.fn()}
      />,
    );
    // Give any (incorrectly scheduled) refresh interval time to fire.
    await new Promise((resolve) => setTimeout(resolve, 100));
    expect(getWorkflowRun).not.toHaveBeenCalled();
  });
});
