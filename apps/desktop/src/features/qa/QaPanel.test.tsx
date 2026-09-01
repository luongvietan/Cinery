import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { QaPanel } from "./QaPanel";
import * as api from "./api";

vi.mock("./api");

const run = {
  id: "qa-1",
  projectId: "project-1",
  assetId: "asset-1",
  assetVersionId: "version-1",
  mediaKind: "image" as const,
  workflowRunId: "workflow-1",
  status: "succeeded" as const,
  overallStatus: "fail" as const,
  adapterId: "mock_visual_qa",
  adapterVersion: "1",
  modelId: "mock-vlm",
  executionLocation: "local",
  checkPlan: {
    schemaVersion: 1 as const,
    assetId: "asset-1",
    assetVersionId: "version-1",
    ownerEntityId: "character-1",
    assetType: "image",
    referenceAssetVersionIds: ["face-v1", "look-v1"],
    checks: [],
    createdAt: "2026-08-28T00:00:00Z",
  },
  contextSnapshot: {},
  rawResponseMetadata: null,
  errorCode: null,
  errorMessage: null,
  createdAt: "2026-08-28T00:00:00Z",
  startedAt: "2026-08-28T00:00:01Z",
  completedAt: "2026-08-28T00:00:02Z",
};

const checks = [
  {
    id: "row-fail",
    qaRunId: "qa-1",
    checkId: "lock:scar",
    checkType: "permanent_visual_lock" as const,
    source: "visual_lock" as const,
    requirement: { label: "Right eyebrow scar" },
    status: "fail" as const,
    confidence: 0.92,
    observed: "Scar appears on character-left.",
    reason: "Wrong side.",
    repairHint: "Move only the scar.",
    reviewStatus: "unreviewed" as const,
    reviewNote: null,
    reviewedAt: null,
    createdAt: run.completedAt,
  },
  {
    id: "row-uncertain",
    qaRunId: "qa-1",
    checkId: "lock:hair",
    checkType: "hair_consistency" as const,
    source: "visual_lock" as const,
    requirement: { label: "Hairline" },
    status: "uncertain" as const,
    confidence: null,
    observed: "Partly obscured.",
    reason: "Insufficient evidence.",
    repairHint: null,
    reviewStatus: "unreviewed" as const,
    reviewNote: null,
    reviewedAt: null,
    createdAt: run.completedAt,
  },
  {
    id: "row-pass",
    qaRunId: "qa-1",
    checkId: "reference:identity",
    checkType: "identity_similarity" as const,
    source: "canonical_reference" as const,
    requirement: { label: "Identity" },
    status: "pass" as const,
    confidence: 0.98,
    observed: "Identity matches.",
    reason: "Facial structure is consistent.",
    repairHint: null,
    reviewStatus: "confirmed" as const,
    reviewNote: null,
    reviewedAt: run.completedAt,
    createdAt: run.completedAt,
  },
];

describe("QaPanel", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(api.listQaRuns).mockResolvedValue([run]);
    vi.mocked(api.getQaRun).mockResolvedValue({ run, checks });
    vi.mocked(api.reviewQaCheck).mockResolvedValue({ run, checks });
  });

  it("groups effective results, exposes provenance, and persists human review", async () => {
    const user = userEvent.setup();
    render(
      <QaPanel
        projectRootPath="/projects/red-door"
        assetVersionId="version-1"
        versionLabel="v001"
      />,
    );

    expect(await screen.findByRole("heading", { name: "Visual QA" })).toBeInTheDocument();
    expect(screen.getByText("FAIL")).toBeInTheDocument();
    expect(within(screen.getByRole("region", { name: "Failed checks" })).getByText("Right eyebrow scar")).toBeInTheDocument();
    expect(within(screen.getByRole("region", { name: "Checks needing review" })).getByText("Hairline")).toBeInTheDocument();
    expect(within(screen.getByRole("region", { name: "Passed checks" })).getByText("Identity")).toBeInTheDocument();
    expect(screen.getByText("LOCAL")).toBeInTheDocument();
    expect(screen.getByText(/face-v1, look-v1/)).toBeInTheDocument();

    const failed = screen.getByRole("region", { name: "Failed checks" });
    await user.type(within(failed).getByLabelText("Review note for Right eyebrow scar"), "False positive");
    await user.click(within(failed).getByRole("button", { name: "Override as Pass" }));

    expect(api.reviewQaCheck).toHaveBeenCalledWith({
      projectRootPath: "/projects/red-door",
      qaRunId: "qa-1",
      checkId: "lock:scar",
      reviewStatus: "overridden_pass",
      note: "False positive",
    });
  });

  it("shows cloud disclosure and requires approval before execution", async () => {
    vi.mocked(api.listQaRuns).mockResolvedValue([]);
    const created = {
      run: { id: "workflow-2", status: "created" },
      steps: [],
      events: [],
      providerExecutions: [],
    } as never;
    const waiting = {
      run: { id: "workflow-2", status: "waiting_for_approval" },
      steps: [
        {
          stepDefinitionId: "compile-request",
          stepType: "compile_request",
          outputJson: JSON.stringify({
            executionLocation: "cloud:api.example",
            adapterId: "openai",
            modelId: "gpt-4o-mini",
            request: { references: [{ assetVersionId: "face-v1" }] },
          }),
        },
      ],
      events: [],
      providerExecutions: [],
    } as never;
    const completed = {
      run: { id: "workflow-2", status: "completed" },
      steps: [],
      events: [],
      providerExecutions: [],
    } as never;
    vi.mocked(api.createVisualQaWorkflow).mockResolvedValue(created);
    vi.mocked(api.advanceQaWorkflow).mockResolvedValueOnce(waiting).mockResolvedValueOnce(completed);
    vi.mocked(api.approveQaWorkflow).mockResolvedValue(waiting);
    const user = userEvent.setup();

    render(
      <QaPanel
        projectRootPath="/projects/red-door"
        assetVersionId="version-1"
        versionLabel="v001"
      />,
    );
    await screen.findByText("No QA history for this version.");
    await user.click(screen.getByRole("button", { name: "Run QA" }));

    expect(await screen.findByText("CLOUD: api.example")).toBeInTheDocument();
    expect(api.approveQaWorkflow).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Approve and Run QA" }));
    expect(api.approveQaWorkflow).toHaveBeenCalledWith(
      "/projects/red-door",
      "workflow-2",
      "approve-qa",
    );
    expect(api.advanceQaWorkflow).toHaveBeenCalledTimes(2);
  });
});
