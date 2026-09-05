import { beforeEach, describe, expect, it, vi } from "vitest";
import { invokeCommand } from "../../lib/tauri";
import {
  approveSequencePreflight,
  beginSequenceReview,
  getSequenceFlow,
  markSequenceReferencesReady,
  prepareSequenceExtension,
  updateSequenceBrief,
} from "./sequenceFlowApi";

vi.mock("../../lib/tauri", () => ({ invokeCommand: vi.fn() }));

const mockedInvoke = vi.mocked(invokeCommand);

describe("sequenceFlowApi", () => {
  beforeEach(() => {
    mockedInvoke.mockReset();
  });

  it("loads the scene's sequence flow without hiding command errors", async () => {
    mockedInvoke.mockResolvedValueOnce({ sceneId: "scene-1" });
    await expect(getSequenceFlow("/project", "scene-1")).resolves.toEqual({
      sceneId: "scene-1",
    });
    expect(invokeCommand).toHaveBeenCalledWith("get_sequence_flow", {
      projectRootPath: "/project",
      sceneId: "scene-1",
    });
  });

  it("saves the director brief through the explicit mutation command", async () => {
    mockedInvoke.mockResolvedValueOnce({ sceneId: "scene-1" });
    await updateSequenceBrief("/project", "scene-1", {
      intent: "Tay counts the exits",
      energy: "elevated",
      targetDurationSeconds: 15,
      creditCap: 800,
    });
    expect(invokeCommand).toHaveBeenCalledWith("update_sequence_brief", {
      projectRootPath: "/project",
      sceneId: "scene-1",
      brief: {
        intent: "Tay counts the exits",
        energy: "elevated",
        targetDurationSeconds: 15,
        creditCap: 800,
      },
    });
  });

  it("reports reference blockers without mutating anything", async () => {
    mockedInvoke.mockResolvedValueOnce({
      flow: null,
      blockers: [{ code: "no_cast", message: "No cast" }],
    });
    await expect(markSequenceReferencesReady("/project", "scene-1")).resolves.toEqual({
      flow: null,
      blockers: [{ code: "no_cast", message: "No cast" }],
    });
    expect(invokeCommand).toHaveBeenCalledWith("mark_sequence_references_ready", {
      projectRootPath: "/project",
      sceneId: "scene-1",
    });
  });

  it("approves the generation preflight with an optional compilation", async () => {
    mockedInvoke.mockResolvedValueOnce({ stage: "prompt_approved" });
    await approveSequencePreflight("/project", "scene-1", null);
    expect(invokeCommand).toHaveBeenCalledWith("approve_sequence_preflight", {
      projectRootPath: "/project",
      sceneId: "scene-1",
      approvedCompilationId: null,
    });
  });

  it("begins review through the explicit command", async () => {
    mockedInvoke.mockResolvedValueOnce({ stage: "in_review" });
    await beginSequenceReview("/project", "scene-1");
    expect(invokeCommand).toHaveBeenCalledWith("begin_sequence_review", {
      projectRootPath: "/project",
      sceneId: "scene-1",
    });
  });

  it("prepares an extension disclosure without executing it", async () => {
    mockedInvoke.mockResolvedValueOnce({ direction: "sequel" });
    await prepareSequenceExtension("/project", "scene-1", "sequel");
    expect(invokeCommand).toHaveBeenCalledWith("prepare_sequence_extension", {
      projectRootPath: "/project",
      sceneId: "scene-1",
      direction: "sequel",
    });
  });
});
