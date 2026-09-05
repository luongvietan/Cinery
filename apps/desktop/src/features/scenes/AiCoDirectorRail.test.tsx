import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { AiCoDirectorRail } from "./AiCoDirectorRail";
import type { SequenceFlow } from "@cinematic/domain";

const approvedFlow: SequenceFlow = {
  sceneId: "scene-1",
  brief: { intent: "Tay counts the exits", energy: "elevated", targetDurationSeconds: 15, creditCap: 800 },
  stage: "prompt_approved",
  approvedCompilationId: "comp-1",
  canonicalShotId: null,
  extensionDirection: null,
  createdAt: "now",
  updatedAt: "now",
};

const blockedReadiness = {
  ready: false,
  blockers: [{ message: "This scene has no shots." }],
};

describe("AiCoDirectorRail", () => {
  it("shows contextual checklist items but has no generate or mutation control", () => {
    render(<AiCoDirectorRail flow={null} readiness={blockedReadiness} activeStage="brief" />);
    expect(screen.getByText(/Lock a director brief/i)).toBeInTheDocument();
    expect(screen.getByText(/This scene has no shots./i)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Generate|Approve|Promote/i })).not.toBeInTheDocument();
  });

  it("suggests the next deliberate step for the flow's stage", () => {
    render(
      <AiCoDirectorRail
        flow={approvedFlow}
        readiness={{ ready: true, blockers: [] }}
        activeStage="review"
      />,
    );
    expect(screen.getByText(/review every candidate before promoting/i)).toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });

  it("never shows more than three suggestions", () => {
    render(
      <AiCoDirectorRail
        flow={null}
        readiness={{
          ready: false,
          blockers: [
            { message: "blocker one" },
            { message: "blocker two" },
            { message: "blocker three" },
            { message: "blocker four" },
            { message: "blocker five" },
          ],
        }}
        activeStage="brief"
      />,
    );
    expect(screen.getAllByRole("listitem").length).toBeLessThanOrEqual(3);
  });
});
