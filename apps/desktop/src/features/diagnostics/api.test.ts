import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../lib/tauri", () => ({
  invokeCommand: vi.fn(),
}));

import { invokeCommand } from "../../lib/tauri";
import { exportDiagnostics, getDiagnosticsFolder } from "./api";

describe("diagnostics api", () => {
  beforeEach(() => {
    vi.mocked(invokeCommand).mockReset();
  });

  it("exports diagnostics for the project root", async () => {
    vi.mocked(invokeCommand).mockResolvedValueOnce({ fileName: "bundle.zip" });

    const result = await exportDiagnostics("C:/project");

    expect(invokeCommand).toHaveBeenCalledWith("export_diagnostics", {
      projectRootPath: "C:/project",
    });
    expect(result).toEqual({ fileName: "bundle.zip" });
  });

  it("returns the diagnostics folder path", async () => {
    vi.mocked(invokeCommand).mockResolvedValueOnce("C:/project/diagnostics");

    const result = await getDiagnosticsFolder("C:/project");

    expect(invokeCommand).toHaveBeenCalledWith("get_diagnostics_folder", {
      projectRootPath: "C:/project",
    });
    expect(result).toBe("C:/project/diagnostics");
  });
});
