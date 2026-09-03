import { beforeEach, describe, expect, it, vi } from "vitest";
import { invokeCommand } from "../../lib/tauri";
import { createVideoQaWorkflow, createVisualQaWorkflow } from "./api";

vi.mock("../../lib/tauri");

describe("QA workflow API", () => {
  beforeEach(() => vi.mocked(invokeCommand).mockReset());

  it("omits providerId and modelId from Visual QA input when nothing was selected", async () => {
    vi.mocked(invokeCommand).mockResolvedValue({});
    await createVisualQaWorkflow("C:/projects/red-door", "version-1", "", "");
    const call = vi.mocked(invokeCommand).mock.calls[0][1] as { input: Record<string, unknown> };
    expect(call.input.providerId).toBeUndefined();
    expect(call.input.modelId).toBeUndefined();
  });

  it("sends an explicit providerId and modelId for Visual QA when selected", async () => {
    vi.mocked(invokeCommand).mockResolvedValue({});
    await createVisualQaWorkflow("C:/projects/red-door", "version-1", "glm-5.3-flash", "glm-5.3-flash");
    const call = vi.mocked(invokeCommand).mock.calls[0][1] as { input: Record<string, unknown> };
    expect(call.input.providerId).toBe("glm-5.3-flash");
    expect(call.input.modelId).toBe("glm-5.3-flash");
  });

  it("omits providerId and modelId from Video QA input when nothing was selected", async () => {
    vi.mocked(invokeCommand).mockResolvedValue({});
    await createVideoQaWorkflow("C:/projects/red-door", "video-v1", "", "");
    const call = vi.mocked(invokeCommand).mock.calls[0][1] as { input: Record<string, unknown> };
    expect(call.input.providerId).toBeUndefined();
    expect(call.input.modelId).toBeUndefined();
  });
});
