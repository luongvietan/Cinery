import { beforeEach, describe, expect, it, vi } from "vitest";
import { invokeCommand } from "../../lib/tauri";
import { createAsset } from "./api";

vi.mock("../../lib/tauri");

describe("asset API", () => {
  beforeEach(() => {
    vi.mocked(invokeCommand).mockReset();
  });

  it("maps the domain asset type to the Tauri assetType argument", async () => {
    vi.mocked(invokeCommand).mockResolvedValue({
      id: "asset-1",
      projectId: "project-1",
      type: "face_lock",
      label: "MARA-FACE",
      ownerEntityId: null,
      canonicalVersionId: null,
      createdAt: "2026-08-28T00:00:00Z",
      updatedAt: "2026-08-28T00:00:00Z",
    });

    await createAsset({
      projectRootPath: "C:/projects/red-door",
      type: "face_lock",
      label: "MARA-FACE",
    });

    expect(invokeCommand).toHaveBeenCalledWith("create_asset", {
      projectRootPath: "C:/projects/red-door",
      assetType: "face_lock",
      label: "MARA-FACE",
    });
  });
});
