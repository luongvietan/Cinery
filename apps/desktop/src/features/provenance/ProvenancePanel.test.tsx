import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import * as apiModule from "./api";
import { ProvenancePanel } from "./ProvenancePanel";

vi.spyOn(apiModule, "getProvenanceGraph");

describe("ProvenancePanel", () => {
  it("displays loading state initially", () => {
    vi.mocked(apiModule.getProvenanceGraph).mockImplementation(
      () => new Promise(() => {}) // Never resolves
    );

    render(
      <ProvenancePanel
        projectRootPath="/test/project"
        targetKind="asset_version"
        targetId="av-123"
      />
    );

    // Should show loading spinner
    expect(screen.getByText("⟳")).toBeInTheDocument();
  });

  it("displays error state on failure", async () => {
    vi.mocked(apiModule.getProvenanceGraph).mockRejectedValue(
      new Error("Database error")
    );

    render(
      <ProvenancePanel
        projectRootPath="/test/project"
        targetKind="asset_version"
        targetId="av-123"
      />
    );

    await waitFor(() => {
      expect(screen.getByText("Error loading provenance")).toBeInTheDocument();
    });
    expect(screen.getByText("Database error")).toBeInTheDocument();
  });

  it("displays provenance graph with nodes and edges", async () => {
    vi.mocked(apiModule.getProvenanceGraph).mockResolvedValue({
      targetId: "av-123",
      nodes: [
        {
          id: "av-123",
          kind: "asset_version",
          label: "Character Face V1",
          timestamp: "2024-08-29T00:00:00Z",
        },
        {
          id: "gen-456",
          kind: "generation",
          label: "Generated artifact",
          timestamp: "2024-08-28T00:00:00Z",
        },
      ],
      edges: [
        {
          from: "gen-456",
          to: "av-123",
          relation: "OUTPUT_OF",
        },
      ],
    });

    render(
      <ProvenancePanel
        projectRootPath="/test/project"
        targetKind="asset_version"
        targetId="av-123"
      />
    );

    // Should display the main node
    await waitFor(() => {
      expect(screen.getByText("Character Face V1")).toBeInTheDocument();
    });

    // Should display node type badge
    expect(screen.getByText("asset_version")).toBeInTheDocument();

    // Should show summary
    expect(screen.getByText(/2 nodes/)).toBeInTheDocument();
  });

  it("calls onNavigate when View button is clicked", async () => {
    const onNavigate = vi.fn();
    const user = userEvent.setup();

    vi.mocked(apiModule.getProvenanceGraph).mockResolvedValue({
      targetId: "av-123",
      nodes: [
        {
          id: "av-123",
          kind: "asset_version",
          label: "Character Face V1",
          timestamp: "2024-08-29T00:00:00Z",
        },
      ],
      edges: [],
    });

    render(
      <ProvenancePanel
        projectRootPath="/test/project"
        targetKind="asset_version"
        targetId="av-123"
        onNavigate={onNavigate}
      />
    );

    await waitFor(() => {
      expect(screen.getByText("Character Face V1")).toBeInTheDocument();
    });

    const viewButton = screen.getByRole("button", { name: "View" });
    await user.click(viewButton);

    expect(onNavigate).toHaveBeenCalledWith("asset_version", "av-123");
  });
});
