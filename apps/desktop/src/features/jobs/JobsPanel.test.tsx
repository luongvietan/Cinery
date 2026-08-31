import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ProviderJobView, ProjectRecoveryState } from "@cinematic/domain";
import { JobsPanel } from "./JobsPanel";

vi.mock("../../lib/tauri", () => ({
  invokeCommand: vi.fn(),
}));
vi.mock("../workflows/api", () => ({
  listProviderJobs: vi.fn(),
}));
vi.mock("../../lib/panelNavigation", () => ({
  openPanel: vi.fn(),
}));

import { invokeCommand } from "../../lib/tauri";
import { listProviderJobs } from "../workflows/api";
import { openPanel } from "../../lib/panelNavigation";

const emptyRecovery: ProjectRecoveryState = {
  projectId: "project-1",
  hasIncompleteJobs: false,
  classifications: [],
};

function job(overrides: Partial<ProviderJobView> = {}): ProviderJobView {
  return {
    id: "job-1",
    providerId: "fake_async_video",
    providerJobId: "fake-job-run-1:execute:1",
    status: "polling",
    progressPercent: 50,
    submittedAt: "2026-08-30T00:00:00Z",
    updatedAt: "2026-08-30T00:00:05Z",
    lastPolledAt: "2026-08-30T00:00:05Z",
    executionId: "attempt-1",
    workflowRunId: "run-1",
    stepDefinitionId: "execute",
    attemptNumber: 1,
    modelId: "fake-video-v1",
    attemptStatus: "running",
    operationId: "scene.generate_video",
    runStatus: "running",
    ...overrides,
  };
}

describe("JobsPanel — durable background provider jobs (P10.1)", () => {
  beforeEach(() => {
    vi.mocked(invokeCommand).mockReset();
    vi.mocked(listProviderJobs).mockReset();
    vi.mocked(openPanel).mockReset();
    vi.mocked(invokeCommand).mockResolvedValue(emptyRecovery);
  });

  it("lists provider jobs with provider, model, status, progress, and attempt", async () => {
    vi.mocked(listProviderJobs).mockResolvedValue([job()]);
    render(<JobsPanel projectRootPath="C:/projects/red-door" />);

    expect(await screen.findByText("fake_async_video")).toBeInTheDocument();
    expect(screen.getByText("fake-video-v1")).toBeInTheDocument();
    expect(screen.getByText("Working")).toBeInTheDocument();
    expect(screen.getByText("50%")).toBeInTheDocument();
    expect(screen.getByText("scene.generate_video")).toBeInTheDocument();
    expect(screen.getByText("1", { selector: "dd" })).toBeInTheDocument();
  });

  it("navigates to the workflow panel from a job card", async () => {
    vi.mocked(listProviderJobs).mockResolvedValue([job()]);
    render(<JobsPanel projectRootPath="C:/projects/red-door" />);

    await userEvent.click(await screen.findByRole("button", { name: "Open workflow" }));
    expect(openPanel).toHaveBeenCalledWith("workflows");
  });

  it("keeps polling while a job is active and stops after everything settles", async () => {
    const listJobs = vi.mocked(listProviderJobs);
    listJobs
      .mockResolvedValueOnce([job()])
      .mockResolvedValueOnce([job({ status: "completed", progressPercent: 100, attemptStatus: "succeeded", runStatus: "completed" })])
      .mockResolvedValue([job({ status: "completed", progressPercent: 100, attemptStatus: "succeeded", runStatus: "completed" })]);
    render(<JobsPanel projectRootPath="C:/projects/red-door" />);

    await waitFor(() => expect(listJobs).toHaveBeenCalledTimes(1));
    // The refresh interval fires because an active job exists.
    await waitFor(() => expect(listJobs.mock.calls.length).toBeGreaterThan(1), { timeout: 4000 });
    expect(screen.getByText("Completed")).toBeInTheDocument();
  });

  it("shows the all-clear state when no jobs and no recovery classifications exist", async () => {
    vi.mocked(listProviderJobs).mockResolvedValue([]);
    render(<JobsPanel projectRootPath="C:/projects/red-door" />);

    expect(await screen.findByText(/All jobs completed/)).toBeInTheDocument();
  });
});
