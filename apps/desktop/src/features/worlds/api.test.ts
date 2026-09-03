import { beforeEach, describe, expect, it, vi } from "vitest";
import { invokeCommand } from "../../lib/tauri";
import { createWorldPlateWorkflowRun } from "./api";

vi.mock("../../lib/tauri");

describe("World Plate workflow API", () => {
  beforeEach(() => vi.mocked(invokeCommand).mockReset());

  it("omits providerId and modelId when nothing was selected", async () => {
    vi.mocked(invokeCommand).mockResolvedValue({});
    await createWorldPlateWorkflowRun("C:/projects/red-door", "world-1", [], "", "");
    const call = vi.mocked(invokeCommand).mock.calls[0][1] as { input: Record<string, unknown> };
    expect(call.input.providerId).toBeUndefined();
    expect(call.input.modelId).toBeUndefined();
  });

  it("sends an explicit providerId and modelId when selected", async () => {
    vi.mocked(invokeCommand).mockResolvedValue({});
    await createWorldPlateWorkflowRun("C:/projects/red-door", "world-1", [], "kira", "kira-3.0-image");
    const call = vi.mocked(invokeCommand).mock.calls[0][1] as { input: Record<string, unknown> };
    expect(call.input.providerId).toBe("kira");
    expect(call.input.modelId).toBe("kira-3.0-image");
  });
});
