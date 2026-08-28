import { beforeEach, describe, expect, it, vi } from "vitest";
import { invokeCommand } from "../../lib/tauri";
import { listGenerationResults, promoteGeneratedArtifact } from "./api";

vi.mock("../../lib/tauri");

describe("production generation API", () => {
  beforeEach(() => vi.mocked(invokeCommand).mockReset());

  it("loads generated result sets for one workflow run", async () => {
    vi.mocked(invokeCommand).mockResolvedValue([]);
    await listGenerationResults("C:/projects/red-door", "run-1");
    expect(invokeCommand).toHaveBeenCalledWith("list_generation_results", {
      projectRootPath: "C:/projects/red-door",
      workflowRunId: "run-1",
    });
  });

  it("keeps promotion explicit and passes canonical choice to the backend", async () => {
    vi.mocked(invokeCommand).mockResolvedValue({});
    await promoteGeneratedArtifact("C:/projects/red-door", "artifact-2", "asset-1", true);
    expect(invokeCommand).toHaveBeenCalledWith("promote_generated_artifact", {
      projectRootPath: "C:/projects/red-door",
      artifactId: "artifact-2",
      targetAssetId: "asset-1",
      setCanonical: true,
    });
  });
});
