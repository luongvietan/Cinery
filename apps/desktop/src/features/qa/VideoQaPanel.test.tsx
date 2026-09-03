import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { WorkflowRunDetail, WorkflowRunRecord } from "@cinematic/domain";
import { VideoQaPanel } from "./VideoQaPanel";
import * as api from "./api";
import { getProviderCapabilities, getProviderConfigurationStatus, getWorkflowRun, listCustomProviders, listProviderModels, listProviders, listWorkflowRuns } from "../workflows/api";
import type { QaCheckRecord, QaRunDetail, QaRunRecord, QaRunStatus } from "./types";

vi.mock("./api");
vi.mock("../workflows/api");

function qaRun(overrides: Partial<QaRunRecord> = {}): QaRunRecord {
  return {
    id: "qa-v1",
    projectId: "project-1",
    assetId: "video-asset",
    assetVersionId: "video-v1",
    mediaKind: "video",
    workflowRunId: "workflow-v1",
    status: "succeeded",
    overallStatus: "fail",
    adapterId: "openai",
    adapterVersion: "1",
    modelId: "video-evaluator",
    executionLocation: "cloud:openai",
    checkPlan: {
      schemaVersion: 1,
      assetId: "video-asset",
      assetVersionId: "video-v1",
      ownerEntityId: "scene-1",
      assetType: "video",
      referenceAssetVersionIds: ["keyframe-v1"],
      checks: [],
      createdAt: "2026-09-01T00:00:00Z",
    },
    contextSnapshot: {},
    rawResponseMetadata: null,
    errorCode: null,
    errorMessage: null,
    createdAt: "2026-09-01T00:00:00Z",
    startedAt: "2026-09-01T00:00:01Z",
    completedAt: "2026-09-01T00:00:02Z",
    ...overrides,
  };
}

function qaCheck(overrides: Partial<QaCheckRecord> = {}): QaCheckRecord {
  return {
    id: "check-row-1",
    qaRunId: "qa-v1",
    checkId: "video:integrity",
    checkType: "video_integrity",
    source: "artifact_detection",
    requirement: { label: "Video integrity" },
    status: "fail",
    confidence: 0.91,
    observed: "A decode discontinuity appears at 00:02.",
    reason: "The stream contains a broken frame boundary.",
    repairHint: null,
    reviewStatus: "unreviewed",
    reviewNote: null,
    reviewedAt: null,
    createdAt: "2026-09-01T00:00:02Z",
    ...overrides,
  };
}

function qaDetail(run = qaRun(), checks = [qaCheck()]): QaRunDetail {
  return { run, checks };
}

function workflowDetail(
  status: WorkflowRunDetail["run"]["status"],
  assetVersionId = "video-v1",
  id = "workflow-v1",
): WorkflowRunDetail {
  return {
    run: {
      id,
      projectId: "project-1",
      skillId: "video-qa",
      skillVersion: "1.0.0",
      operationId: "asset.run_video_qa",
      status,
      inputJson: JSON.stringify({ assetVersionId, adapterId: "openai" }),
      prerequisiteReportJson: null,
      contextSnapshotJson: null,
      currentStepIndex: 4,
      failureCode: status === "failed" ? "QA_ADAPTER_FAILED" : null,
      failureMessage: status === "failed" ? "Video evaluation failed." : null,
      createdAt: "2026-09-01T00:00:00Z",
      updatedAt: "2026-09-01T00:00:01Z",
      completedAt: ["completed", "failed", "cancelled", "rejected"].includes(status)
        ? "2026-09-01T00:00:02Z"
        : null,
    },
    steps: status === "running" ? [{
      id: "execute-step",
      workflowRunId: id,
      stepDefinitionId: "execute",
      stepIndex: 4,
      stepType: "execute",
      status: "running",
      inputJson: null,
      outputJson: null,
      startedAt: "2026-09-01T00:00:01Z",
      completedAt: null,
    }] : [],
    events: [],
  };
}

function workflowRecord(detail: WorkflowRunDetail): WorkflowRunRecord {
  return detail.run;
}

describe("VideoQaPanel", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(api.listQaRuns).mockResolvedValue([]);
    vi.mocked(listWorkflowRuns).mockResolvedValue([]);
    vi.mocked(api.getQaRun).mockResolvedValue(qaDetail());
    vi.mocked(api.reviewQaCheck).mockResolvedValue(qaDetail());
    vi.mocked(api.createVideoQaWorkflow).mockResolvedValue(workflowDetail("created"));
    vi.mocked(api.advanceQaWorkflow).mockResolvedValue(workflowDetail("waiting_for_approval"));
    vi.mocked(getWorkflowRun).mockResolvedValue(workflowDetail("running"));
  });

  it("is visible for one exact completed video candidate and does not start QA automatically", async () => {
    render(<VideoQaPanel projectRootPath="C:/project" assetVersionId="video-v1" versionLabel="V01" />);

    expect(await screen.findByRole("region", { name: "Video QA for V01" })).toBeInTheDocument();
    expect(screen.getByText("Exact candidate video-v1")).toBeInTheDocument();
    expect(api.listQaRuns).toHaveBeenCalledWith("C:/project", "video-v1");
    expect(api.createVideoQaWorkflow).not.toHaveBeenCalled();
  });

  it("discloses cloud provider, model, transfer mode, and immutable references before approval", async () => {
    const pending = workflowDetail("waiting_for_approval");
    pending.steps = [{
      id: "compile-request",
      workflowRunId: pending.run.id,
      stepDefinitionId: "compile-request",
      stepIndex: 2,
      stepType: "compile_request",
      status: "completed",
      inputJson: null,
      outputJson: JSON.stringify({
        executionLocation: "cloud:openai",
        adapterId: "openai",
        modelId: "video-evaluator",
        evidenceMode: "direct_video",
        request: { references: [{ assetVersionId: "kf-v1" }, { assetVersionId: "look-v3" }] },
      }),
      startedAt: "2026-09-01T00:00:00Z",
      completedAt: "2026-09-01T00:00:01Z",
    }];
    vi.mocked(listWorkflowRuns).mockResolvedValue([workflowRecord(pending)]);
    vi.mocked(getWorkflowRun).mockResolvedValue(pending);

    render(<VideoQaPanel projectRootPath="C:/project" assetVersionId="video-v1" versionLabel="V01" />);

    const approval = await screen.findByRole("region", { name: "Video QA execution review" });
    expect(approval).toHaveTextContent("CLOUD: openai");
    expect(approval).toHaveTextContent("openai · video-evaluator");
    expect(approval).toHaveTextContent("Direct video transfer");
    expect(approval).toHaveTextContent("kf-v1, look-v3");
  });

  it.each([
    ["queued", null, "QUEUED"],
    ["running", null, "RUNNING"],
    ["succeeded", "pass", "PASS"],
    ["succeeded", "fail", "FAIL"],
    ["succeeded", "needs_review", "NEEDS REVIEW"],
    ["failed", null, "FAILED"],
    ["cancelled", null, "CANCELLED"],
  ] satisfies Array<[QaRunStatus, QaRunRecord["overallStatus"], string]>)(
    "shows the %s QA status as %s",
    async (status, overallStatus, expected) => {
      const run = qaRun({ status, overallStatus });
      vi.mocked(api.listQaRuns).mockResolvedValue([run]);
      vi.mocked(api.getQaRun).mockResolvedValue(qaDetail(run, status === "succeeded" ? [qaCheck()] : []));

      render(<VideoQaPanel projectRootPath="C:/project" assetVersionId="video-v1" versionLabel="V01" />);

      expect(await screen.findByTestId("video-qa-effective-overall")).toHaveTextContent(expected);
    },
  );

  it("restores the matching active workflow and disables duplicate QA actions", async () => {
    const active = workflowDetail("running");
    vi.mocked(listWorkflowRuns).mockResolvedValue([
      workflowRecord(workflowDetail("running", "video-v2", "other-workflow")),
      workflowRecord(active),
    ]);
    vi.mocked(getWorkflowRun).mockResolvedValue(active);

    render(<VideoQaPanel projectRootPath="C:/project" assetVersionId="video-v1" versionLabel="V01" />);

    expect(await screen.findByText("Generating…")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Run Video QA" })).toBeDisabled();
    expect(getWorkflowRun).toHaveBeenCalledWith("C:/project", "workflow-v1");
  });

  it("creates only one workflow during rapid clicks", async () => {
    let resolveCreation!: (detail: WorkflowRunDetail) => void;
    vi.mocked(api.createVideoQaWorkflow).mockImplementation(
      () => new Promise((resolve) => { resolveCreation = resolve; }),
    );
    const user = userEvent.setup();
    render(<VideoQaPanel projectRootPath="C:/project" assetVersionId="video-v1" versionLabel="V01" />);
    const button = await screen.findByRole("button", { name: "Run Video QA" });

    const clicks = Promise.all([user.click(button), user.click(button)]);
    await waitFor(() => expect(api.createVideoQaWorkflow).toHaveBeenCalledTimes(1));
    resolveCreation(workflowDetail("created"));
    await clicks;

    expect(api.createVideoQaWorkflow).toHaveBeenCalledWith("C:/project", "video-v1", "", "");
  });

  it("sends the selected provider and model when running Video QA", async () => {
    vi.mocked(listProviders).mockResolvedValue(["glm-5.3-flash"]);
    vi.mocked(listCustomProviders).mockResolvedValue([{
      providerId: "glm-5.3-flash", displayName: "GLM 5.3 Flash", baseUrl: "https://api.example.test/v1",
      purpose: "llm", models: [{ id: "glm-5.3-flash", name: "GLM 5.3 Flash" }], headers: [],
    }]);
    vi.mocked(listProviderModels).mockResolvedValue(["glm-5.3-flash"]);
    vi.mocked(getProviderCapabilities).mockRejectedValue(new Error("custom provider"));
    vi.mocked(getProviderConfigurationStatus).mockResolvedValue({
      providerId: "glm-5.3-flash", enabled: true, credentialConfigured: true,
      defaultModel: "glm-5.3-flash", models: ["glm-5.3-flash"],
    });
    vi.mocked(api.createVideoQaWorkflow).mockResolvedValue(workflowDetail("created"));
    const user = userEvent.setup();

    render(<VideoQaPanel projectRootPath="C:/project" assetVersionId="video-v1" versionLabel="V01" />);
    await screen.findAllByDisplayValue("glm-5.3-flash");
    const button = await screen.findByRole("button", { name: "Run Video QA" });
    await user.click(button);

    expect(api.createVideoQaWorkflow).toHaveBeenCalledWith("C:/project", "video-v1", "glm-5.3-flash", "glm-5.3-flash");
  });

  it("requires explicit Video QA approval before evaluator execution", async () => {
    const waiting = workflowDetail("waiting_for_approval");
    const ready = workflowDetail("ready_for_execution");
    const completed = workflowDetail("completed");
    vi.mocked(api.advanceQaWorkflow).mockResolvedValueOnce(waiting).mockResolvedValueOnce(completed);
    vi.mocked(api.approveQaWorkflow).mockResolvedValue(ready);
    const user = userEvent.setup();
    render(<VideoQaPanel projectRootPath="C:/project" assetVersionId="video-v1" versionLabel="V01" />);

    await user.click(await screen.findByRole("button", { name: "Run Video QA" }));
    expect(await screen.findByRole("button", { name: "Approve and Run Video QA" })).toBeInTheDocument();
    expect(api.approveQaWorkflow).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Approve and Run Video QA" }));

    expect(api.approveQaWorkflow).toHaveBeenCalledWith(
      "C:/project",
      "workflow-v1",
      "approve-video-qa",
      "Video QA evidence disclosure reviewed",
    );
    expect(api.advanceQaWorkflow).toHaveBeenCalledTimes(2);
  });

  it("keeps the approved state visible when advancing after approval fails", async () => {
    const waiting = workflowDetail("waiting_for_approval");
    const ready = workflowDetail("ready_for_execution");
    vi.mocked(api.advanceQaWorkflow).mockResolvedValueOnce(waiting).mockRejectedValueOnce(new Error("advance failed"));
    vi.mocked(api.approveQaWorkflow).mockResolvedValue(ready);
    const user = userEvent.setup();
    render(<VideoQaPanel projectRootPath="C:/project" assetVersionId="video-v1" versionLabel="V01" />);

    await user.click(await screen.findByRole("button", { name: "Run Video QA" }));
    await user.click(await screen.findByRole("button", { name: "Approve and Run Video QA" }));

    expect(api.approveQaWorkflow).toHaveBeenCalledTimes(1);
    expect(await screen.findByRole("alert")).toHaveTextContent("advance failed");
    // The stale "waiting_for_approval" screen must not reappear -- retrying
    // it would resubmit the same (already-decided) approval.
    expect(screen.queryByRole("button", { name: "Approve and Run Video QA" })).not.toBeInTheDocument();
  });

  it("allows an explicit rerun after terminal workflow history", async () => {
    vi.mocked(listWorkflowRuns).mockResolvedValue([workflowRecord(workflowDetail("completed"))]);
    vi.mocked(getWorkflowRun).mockResolvedValue(workflowDetail("completed"));
    const user = userEvent.setup();
    render(<VideoQaPanel projectRootPath="C:/project" assetVersionId="video-v1" versionLabel="V01" />);

    const button = await screen.findByRole("button", { name: "Run Video QA" });
    await waitFor(() => expect(button).toBeEnabled());
    await user.click(button);

    expect(api.createVideoQaWorkflow).toHaveBeenCalledTimes(1);
  });

  it("keeps V1 history version-local after V2 exists", async () => {
    const v1 = qaRun({ id: "qa-v1", assetVersionId: "video-v1", createdAt: "2026-09-01T00:00:00Z" });
    const v2 = qaRun({ id: "qa-v2", assetVersionId: "video-v2", overallStatus: "pass", createdAt: "2026-09-02T00:00:00Z" });
    vi.mocked(api.listQaRuns).mockImplementation(async (_root, version) => version === "video-v1" ? [v1] : [v2]);
    vi.mocked(api.getQaRun).mockImplementation(async (_root, id) => id === "qa-v1" ? qaDetail(v1) : qaDetail(v2, [qaCheck({ qaRunId: "qa-v2", status: "pass" })]));

    render(<>
      <VideoQaPanel projectRootPath="C:/project" assetVersionId="video-v1" versionLabel="V01" />
      <VideoQaPanel projectRootPath="C:/project" assetVersionId="video-v2" versionLabel="V02" />
    </>);

    const v1Panel = await screen.findByRole("region", { name: "Video QA for V01" });
    const v2Panel = await screen.findByRole("region", { name: "Video QA for V02" });
    expect(within(v1Panel).getByText("qa-v1")).toBeInTheDocument();
    expect(within(v1Panel).queryByText("qa-v2")).not.toBeInTheDocument();
    expect(within(v2Panel).getByText("qa-v2")).toBeInTheDocument();
  });

  it("shows raw evaluator evidence, human override, and effective status separately", async () => {
    const run = qaRun({ overallStatus: "pass" });
    const overridden = qaCheck({
      status: "fail",
      reviewStatus: "overridden_pass",
      reviewNote: "False positive after frame review",
      reviewedAt: "2026-09-01T01:00:00Z",
    });
    vi.mocked(api.listQaRuns).mockResolvedValue([run]);
    vi.mocked(api.getQaRun).mockResolvedValue(qaDetail(run, [overridden]));

    render(<VideoQaPanel projectRootPath="C:/project" assetVersionId="video-v1" versionLabel="V01" />);

    const check = await screen.findByRole("listitem", { name: "Video integrity QA finding" });
    expect(within(check).getByText(/Evaluator finding:/)).toHaveTextContent("FAIL");
    expect(within(check).getByText(/Human decision:/)).toHaveTextContent("OVERRIDDEN PASS");
    expect(within(check).getByText(/Effective status:/)).toHaveTextContent("PASS");
    expect(within(check).getByText(/False positive after frame review/)).toBeInTheDocument();
    expect(within(check).getByText(/decode discontinuity/)).toBeInTheDocument();
  });

  it("persists a human review without replacing the raw evaluator finding", async () => {
    const run = qaRun();
    const original = qaCheck();
    const reviewed = qaCheck({ reviewStatus: "overridden_pass", reviewNote: "Reviewed frame by frame" });
    vi.mocked(api.listQaRuns).mockResolvedValue([run]);
    vi.mocked(api.getQaRun).mockResolvedValue(qaDetail(run, [original]));
    vi.mocked(api.reviewQaCheck).mockResolvedValue(qaDetail({ ...run, overallStatus: "pass" }, [reviewed]));
    const user = userEvent.setup();
    render(<VideoQaPanel projectRootPath="C:/project" assetVersionId="video-v1" versionLabel="V01" />);

    const check = await screen.findByRole("listitem", { name: "Video integrity QA finding" });
    await user.type(within(check).getByLabelText("Review note for Video integrity"), "Reviewed frame by frame");
    await user.click(within(check).getByRole("button", { name: "Override as Pass" }));

    expect(api.reviewQaCheck).toHaveBeenCalledWith({
      projectRootPath: "C:/project",
      qaRunId: "qa-v1",
      checkId: "video:integrity",
      reviewStatus: "overridden_pass",
      note: "Reviewed frame by frame",
    });
    expect(await within(check).findByText(/Evaluator finding:/)).toHaveTextContent("FAIL");
    expect(within(check).getByText(/Effective status:/)).toHaveTextContent("PASS");
  });
});
