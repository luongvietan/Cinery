import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DiagnosticsPanel } from "./DiagnosticsPanel";
import * as api from "./api";
import type { DiagnosticsBundle } from "@cinematic/domain";

vi.mock("./api", () => ({
  exportDiagnostics: vi.fn(),
  getDiagnosticsFolder: vi.fn(),
}));

const bundle: DiagnosticsBundle = {
  fileName: "cinery-diagnostics-2026-08-28T10-00-00Z.zip",
  exportedAt: "2026-08-28T10:00:00Z",
  outputPath: "C:\\project\\diagnostics",
  files: [
    { name: "logs.txt", content: "log line without secrets" },
    { name: "app-version.json", content: "{\n  \"version\": \"0.0.0\"\n}" },
  ],
};

describe("DiagnosticsPanel", () => {
  beforeEach(() => {
    vi.mocked(api.getDiagnosticsFolder).mockResolvedValue("C:/project/diagnostics");
    vi.mocked(api.exportDiagnostics).mockResolvedValue(bundle);
  });

  it("shows the diagnostics folder path", async () => {
    render(<DiagnosticsPanel projectRootPath={"C:/project"} />);

    await waitFor(() => {
      expect(screen.getByText((content) => content.includes("C:/project/diagnostics"))).toBeInTheDocument();
    });
  });

  it("exports a bundle and lists its files", async () => {
    render(<DiagnosticsPanel projectRootPath={"C:/project"} />);

    fireEvent.click(screen.getByRole("button", { name: "Export diagnostics bundle" }));

    await waitFor(() => {
      expect(screen.getByRole("status")).toBeInTheDocument();
    });
    expect(api.exportDiagnostics).toHaveBeenCalledWith("C:/project");
    expect(screen.getByText("app-version.json")).toBeInTheDocument();
    expect(screen.getByText("logs.txt")).toBeInTheDocument();
  });

  it("renders an error when export fails", async () => {
    vi.mocked(api.exportDiagnostics).mockRejectedValueOnce(
      new Error("diagnostics failed"),
    );

    render(<DiagnosticsPanel projectRootPath={"C:/project"} />);

    fireEvent.click(screen.getByRole("button", { name: "Export diagnostics bundle" }));

    await waitFor(() => {
      expect(screen.getByRole("alert")).toBeInTheDocument();
    });
  });
});
