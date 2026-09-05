import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SequenceExtend } from "./SequenceExtend";
import { prepareSequenceExtension } from "./sequenceFlowApi";
import { resolveCanonicalShotVideo } from "./api";
import type { ExtensionRequest, SequenceFlow } from "@cinematic/domain";

vi.mock("./sequenceFlowApi", () => ({
  prepareSequenceExtension: vi.fn(),
}));
vi.mock("./api", () => ({
  resolveCanonicalShotVideo: vi.fn(),
}));

function flowWith(overrides: Partial<SequenceFlow>): SequenceFlow {
  return {
    sceneId: "scene-1",
    brief: { intent: "Tay counts the exits", energy: "elevated", targetDurationSeconds: 15, creditCap: 800 },
    stage: "canonical_selected",
    approvedCompilationId: "comp-1",
    canonicalShotId: null,
    extensionDirection: null,
    createdAt: "now",
    updatedAt: "now",
    ...overrides,
  };
}

const noCanonicalProps = {
  projectRootPath: "/project",
  sceneId: "scene-1",
  flow: flowWith({ canonicalShotId: null }),
  onChanged: vi.fn(),
};

const prepared: ExtensionRequest = {
  sceneId: "scene-1",
  shotId: "shot-1",
  direction: "sequel",
  canonicalVideoAssetVersionId: "video-v1",
  carriedLocks: { speech: "calm", movement: "precise", stillness: "restrained" },
  worldContinuity: { plateId: "plate-1", plateAssetVersionId: "plate-v1", description: "The Station" },
  continuationPrompt: "Continue the laundromat scene after the bell rings.",
};

describe("SequenceExtend", () => {
  beforeEach(() => {
    vi.mocked(prepareSequenceExtension).mockReset();
    vi.mocked(resolveCanonicalShotVideo).mockReset();
  });

  it("does not allow extension without a canonical source and a direction", () => {
    render(<SequenceExtend {...noCanonicalProps} />);
    expect(screen.getByRole("button", { name: "Prepare extension" })).toBeDisabled();
    expect(prepareSequenceExtension).not.toHaveBeenCalled();
  });

  it("prepares an explicit sequel request from the exact canonical pin", async () => {
    vi.mocked(resolveCanonicalShotVideo).mockResolvedValue("video-v1");
    vi.mocked(prepareSequenceExtension).mockResolvedValue(prepared);
    const onChanged = vi.fn();
    const user = userEvent.setup();
    render(
      <SequenceExtend
        projectRootPath="/project"
        sceneId="scene-1"
        flow={flowWith({ canonicalShotId: "shot-1" })}
        onChanged={onChanged}
      />,
    );
    expect(await screen.findByText(/exact version video-v1/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Prepare extension" })).toBeDisabled();

    await user.click(screen.getByRole("radio", { name: /After this clip/ }));
    await user.click(screen.getByRole("button", { name: "Prepare extension" }));

    expect(prepareSequenceExtension).toHaveBeenCalledWith("/project", "scene-1", "sequel");
    expect(await screen.findByText(/nothing has been generated and no credits spent/i)).toBeInTheDocument();
    expect(screen.getByText(/Continue the laundromat scene/)).toBeInTheDocument();
    expect(screen.getByText(/World plate: plate-v1/)).toBeInTheDocument();
    expect(onChanged).toHaveBeenCalledTimes(1);
  });
});
