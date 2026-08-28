import { render, screen, waitFor } from "@testing-library/react";
import type { ProjectHealthIssue } from "@cinematic/domain";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProjectHealthPanel } from "./ProjectHealthPanel";
import { getProjectHealth } from "./healthApi";

vi.mock("./healthApi", () => ({ getProjectHealth: vi.fn() }));

describe("ProjectHealthPanel", () => {
  beforeEach(() => {
    vi.mocked(getProjectHealth).mockReset();
  });

  it("shows loading state initially", () => {
    vi.mocked(getProjectHealth).mockImplementation(
      () => new Promise(() => {
        /* never resolves */
      })
    );

    render(<ProjectHealthPanel projectRootPath="/projects/test" />);

    expect(document.querySelector(".animate-pulse")).toBeInTheDocument();
  });

  it("shows clean status when no issues are found", async () => {
    vi.mocked(getProjectHealth).mockResolvedValue([]);

    render(<ProjectHealthPanel projectRootPath="/projects/test" />);

    await waitFor(() => {
      expect(screen.getByText("✓ Clean")).toBeInTheDocument();
    });

    expect(
      screen.getByText(
        "No integrity issues detected. Your project is in good health."
      )
    ).toBeInTheDocument();
  });

  it("displays error issues with details", async () => {
    const issues: ProjectHealthIssue[] = [
      {
        code: "BROKEN_ASSET_FILE_REFERENCE",
        severity: "error",
        entityType: "asset_version",
        entityId: "asset-v1",
        message: "Asset media file is missing.",
        remediation: "Restore the media file.",
      },
    ];

    vi.mocked(getProjectHealth).mockResolvedValue(issues);

    render(<ProjectHealthPanel projectRootPath="/projects/test" />);

    await waitFor(() => {
      expect(
        screen.getByText("BROKEN_ASSET_FILE_REFERENCE")
      ).toBeInTheDocument();
    });

    expect(
      screen.getByText("Asset media file is missing.")
    ).toBeInTheDocument();
    expect(screen.getByText("Restore the media file.")).toBeInTheDocument();
    expect(screen.getByText("Entity: asset-v1")).toBeInTheDocument();
  });

  it("shows summary counts for multiple issues", async () => {
    const issues: ProjectHealthIssue[] = [
      {
        code: "ERROR_CODE",
        severity: "error",
        entityType: "asset_version",
        entityId: "id1",
        message: "Error 1",
        remediation: null,
      },
      {
        code: "WARNING_CODE",
        severity: "warning",
        entityType: "scene",
        entityId: "scene1",
        message: "Warning 1",
        remediation: null,
      },
      {
        code: "INFO_CODE",
        severity: "info",
        entityType: "asset",
        entityId: "asset1",
        message: "Info 1",
        remediation: null,
      },
    ];

    vi.mocked(getProjectHealth).mockResolvedValue(issues);

    render(<ProjectHealthPanel projectRootPath="/projects/test" />);

    await waitFor(() => {
      expect(screen.getByText(/1 error, 1 warning, 1 info/)).toBeInTheDocument();
    });
  });

  it("shows fatal severity when present", async () => {
    const issues: ProjectHealthIssue[] = [
      {
        code: "FATAL_CODE",
        severity: "fatal",
        entityType: "project",
        entityId: null,
        message: "Project database schema is too new.",
        remediation: null,
      },
    ];

    vi.mocked(getProjectHealth).mockResolvedValue(issues);

    render(<ProjectHealthPanel projectRootPath="/projects/test" />);

    await waitFor(() => {
      expect(screen.getByText("1 fatal")).toBeInTheDocument();
    });
  });

  it("handles scan errors gracefully", async () => {
    vi.mocked(getProjectHealth).mockRejectedValue(
      new Error("Network error")
    );

    render(<ProjectHealthPanel projectRootPath="/projects/test" />);

    await waitFor(() => {
      expect(screen.getByText(/Health scan error: Network error/)).toBeInTheDocument();
    });
  });

  it("shows info issues with blue styling", async () => {
    const issues: ProjectHealthIssue[] = [
      {
        code: "SUPERSEDED_REFERENCE",
        severity: "info",
        entityType: "scene",
        entityId: "scene1",
        message: "Scene references superseded World V01. Current canonical is V03.",
        remediation: null,
      },
    ];

    vi.mocked(getProjectHealth).mockResolvedValue(issues);

    render(<ProjectHealthPanel projectRootPath="/projects/test" />);

    await waitFor(() => {
      expect(screen.getByText("SUPERSEDED_REFERENCE")).toBeInTheDocument();
    });

    const badge = screen.getByText("info");
    expect(badge).toHaveClass("bg-blue-200");
  });

  it("allows null remediation", async () => {
    const issues: ProjectHealthIssue[] = [
      {
        code: "TEST_ISSUE",
        severity: "warning",
        entityType: "asset",
        entityId: null,
        message: "This is a test issue.",
        remediation: null,
      },
    ];

    vi.mocked(getProjectHealth).mockResolvedValue(issues);

    render(<ProjectHealthPanel projectRootPath="/projects/test" />);

    await waitFor(() => {
      expect(screen.getByText("TEST_ISSUE")).toBeInTheDocument();
    });

    // Remediation text should not appear
    expect(
      screen.queryByText("This should not appear")
    ).not.toBeInTheDocument();
  });
});
