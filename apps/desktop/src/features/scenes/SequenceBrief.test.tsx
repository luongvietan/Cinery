import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SequenceBrief } from "./SequenceBrief";
import { updateSequenceBrief } from "./sequenceFlowApi";
import type { SequenceFlow } from "@cinematic/domain";

vi.mock("./sequenceFlowApi", () => ({
  getSequenceFlow: vi.fn(),
  updateSequenceBrief: vi.fn(),
  markSequenceReferencesReady: vi.fn(),
  approveSequencePreflight: vi.fn(),
  beginSequenceReview: vi.fn(),
  prepareSequenceExtension: vi.fn(),
}));

const lockedFlow: SequenceFlow = {
  sceneId: "scene-1",
  brief: {
    intent: "Tay notices the door",
    energy: "elevated",
    targetDurationSeconds: 15,
    creditCap: 800,
  },
  stage: "brief_locked",
  approvedCompilationId: null,
  canonicalShotId: null,
  extensionDirection: null,
  createdAt: "now",
  updatedAt: "now",
};

describe("SequenceBrief", () => {
  beforeEach(() => {
    vi.mocked(updateSequenceBrief).mockReset();
  });

  it("does not lock the director brief until intent, duration, and credit cap are valid", async () => {
    render(
      <SequenceBrief projectRootPath="/project" sceneId="scene-1" flow={null} onChanged={vi.fn()} />,
    );
    expect(screen.getByRole("button", { name: "Lock brief" })).toBeDisabled();
    const user = userEvent.setup();
    await user.type(screen.getByLabelText("Creative intent"), "A tired man hears a bell");
    await user.click(screen.getByRole("button", { name: "Lock brief" }));
    expect(updateSequenceBrief).toHaveBeenCalledWith(
      "/project",
      "scene-1",
      expect.objectContaining({ intent: "A tired man hears a bell" }),
    );
  });

  it("prefills the locked brief and saves edits while the flow allows them", async () => {
    const onChanged = vi.fn();
    vi.mocked(updateSequenceBrief).mockResolvedValueOnce(lockedFlow);
    render(
      <SequenceBrief
        projectRootPath="/project"
        sceneId="scene-1"
        flow={lockedFlow}
        onChanged={onChanged}
      />,
    );
    const intent = screen.getByLabelText("Creative intent") as HTMLTextAreaElement;
    expect(intent.value).toBe("Tay notices the door");
    expect(screen.getByText(/brief_locked/)).toBeInTheDocument();

    const user = userEvent.setup();
    await user.clear(intent);
    await user.type(intent, "Tay counts the exits");
    await user.click(screen.getByRole("button", { name: "Lock brief" }));
    expect(updateSequenceBrief).toHaveBeenCalledWith(
      "/project",
      "scene-1",
      expect.objectContaining({ intent: "Tay counts the exits" }),
    );
    expect(onChanged).toHaveBeenCalledTimes(1);
  });

  it("surfaces command failures without clearing the draft", async () => {
    vi.mocked(updateSequenceBrief).mockRejectedValueOnce({
      code: "WORKFLOW_INVALID_TRANSITION",
      message: "The director brief can only be edited while the sequence is at brief_locked",
    });
    render(
      <SequenceBrief
        projectRootPath="/project"
        sceneId="scene-1"
        flow={lockedFlow}
        onChanged={vi.fn()}
      />,
    );
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Lock brief" }));
    expect(
      await screen.findByRole("alert"),
    ).toHaveTextContent("The director brief can only be edited while the sequence is at brief_locked");
    expect(screen.queryByText(/Sequence saved/)).not.toBeInTheDocument();
  });
});
