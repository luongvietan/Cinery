import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SequencePreflight } from "./SequencePreflight";
import { approveSequencePreflight } from "./sequenceFlowApi";
import type { SequenceFlow, SequencePreflight as SequencePreflightData } from "@cinematic/domain";

vi.mock("./sequenceFlowApi", () => ({
  approveSequencePreflight: vi.fn(),
  getSequenceFlow: vi.fn(),
  updateSequenceBrief: vi.fn(),
  markSequenceReferencesReady: vi.fn(),
  beginSequenceReview: vi.fn(),
  prepareSequenceExtension: vi.fn(),
}));

function preflightData(overrides: Partial<SequencePreflightData>): SequencePreflightData {
  return {
    sceneId: "scene-1",
    compilation: {
      id: "comp-1",
      sceneId: "scene-1",
      totalDurationSeconds: 8,
      shots: [],
      behavioralLocks: {},
      worldContinuity: {},
      continuity: "",
      providerPrompt: "CINEMA PROMPT — hold the laundromat close-up for 8 seconds",
    } as unknown as SequencePreflightData["compilation"],
    providerPrompt: "CINEMA PROMPT — hold the laundromat close-up for 8 seconds",
    references: [{ assetId: "asset-1", versionId: "v1", role: "world plate" }],
    estimatedCredits: 0,
    runtimeRecommendation: "Within Joey's recommended ~15-second prompt unit",
    canGenerate: true,
    blockers: [],
    ...overrides,
  };
}

const approvedFlow: SequenceFlow = {
  sceneId: "scene-1",
  brief: { intent: "Tay counts the exits", energy: "elevated", targetDurationSeconds: 15, creditCap: 800 },
  stage: "references_ready",
  approvedCompilationId: null,
  canonicalShotId: null,
  extensionDirection: null,
  createdAt: "now",
  updatedAt: "now",
};

const blockedProps = {
  projectRootPath: "/project",
  sceneId: "scene-1",
  flow: approvedFlow,
  onChanged: vi.fn(),
  preflight: preflightData({
    canGenerate: false,
    blockers: [{ code: "world_reference_missing", message: "Missing scene plate: assign a World whose plate has a canonical version" }],
  }),
};

const readyProps = {
  projectRootPath: "/project",
  sceneId: "scene-1",
  flow: approvedFlow,
  onChanged: vi.fn(),
  preflight: preflightData({}),
};

describe("SequencePreflight", () => {
  beforeEach(() => {
    vi.mocked(approveSequencePreflight).mockReset();
  });

  it("shows every disclosure and prevents approval when a required reference is missing", () => {
    render(<SequencePreflight {...blockedProps} />);
    expect(screen.getByText(/Missing scene plate/i)).toBeInTheDocument();
    expect(screen.getByText(blockedProps.preflight.providerPrompt)).toBeInTheDocument();
    expect(screen.getByText(/world plate/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Approve generation" })).toBeDisabled();
    expect(approveSequencePreflight).not.toHaveBeenCalled();
  });

  it("requires an explicit approval before render is enabled", async () => {
    vi.mocked(approveSequencePreflight).mockResolvedValueOnce({ ...approvedFlow, stage: "prompt_approved" });
    render(<SequencePreflight {...readyProps} />);
    expect(screen.getByText(readyProps.preflight.providerPrompt)).toBeInTheDocument();
    expect(screen.getByText(/8s/)).toBeInTheDocument();
    expect(screen.getByText(/Joey/i)).toBeInTheDocument();
    expect(screen.getByText(/not reported/i)).toBeInTheDocument();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Approve generation" }));
    expect(approveSequencePreflight).toHaveBeenCalledWith("/project", "scene-1", "comp-1");
    expect(readyProps.onChanged).toHaveBeenCalledTimes(1);
  });

  it("surfaces a failed approval without clearing the disclosure", async () => {
    vi.mocked(approveSequencePreflight).mockRejectedValueOnce({
      code: "SEQUENCE_FLOW_STAGE_CONFLICT",
      message: "The sequence flow changed before your action completed",
    });
    render(<SequencePreflight {...readyProps} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Approve generation" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The sequence flow changed before your action completed",
    );
  });
});
