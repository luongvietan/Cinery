import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ProjectOverview as ProjectOverviewData } from "@cinematic/domain";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProjectOverview } from "./ProjectOverview";
import { getProjectOverview } from "./api";

vi.mock("./api", () => ({ getProjectOverview: vi.fn() }));

const overview: ProjectOverviewData = {
  readiness: {
    status: "pending",
    nextAction: { id: "story_canon", title: "Story Canon", destination: "canon" },
    steps: [
      { id: "story_canon", title: "Story Canon", status: "pending", detail: "Create the story foundation before production.", action: { id: "story_canon", title: "Story Canon", destination: "canon" } },
      { id: "face_lock", title: "Canonical Face", status: "complete", detail: "Every character has a canonical face reference.", action: null },
    ],
  },
  healthSummary: { openProtectedTbdCount: 0, openTbdCount: 0, activeJobCount: 0 },
  recentActivity: [],
  activeJobs: [],
};

describe("ProjectOverview", () => {
  beforeEach(() => {
    vi.mocked(getProjectOverview).mockReset().mockResolvedValue(overview);
  });

  it("shows backend-derived progress and takes the next action to its existing workspace", async () => {
    const onNavigate = vi.fn();
    render(<ProjectOverview projectRootPath="/projects/red-door" onNavigate={onNavigate} />);

    expect(await screen.findByRole("heading", { name: "Production Progress" })).toBeInTheDocument();
    expect(screen.getByText("Create the story foundation before production.")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Open Story Canon" }));

    expect(onNavigate).toHaveBeenCalledWith("canon");
  });

  it("makes a protected TBD blocker explicit rather than presenting a ready action", async () => {
    vi.mocked(getProjectOverview).mockResolvedValue({
      ...overview,
      readiness: {
        ...overview.readiness,
        status: "blocked",
        nextAction: { id: "resolve_protected_tbd", title: "Resolve protected TBD", destination: "canon" },
        steps: [{ id: "cinema_compilation", title: "Cinema Compilation", status: "blocked", detail: "A protected open TBD blocks this scene from compilation.", action: { id: "resolve_protected_tbd", title: "Resolve protected TBD", destination: "canon" } }],
      },
      healthSummary: { openProtectedTbdCount: 1, openTbdCount: 1, activeJobCount: 0 },
    });

    render(<ProjectOverview projectRootPath="/projects/red-door" onNavigate={vi.fn()} />);

    expect(await screen.findByText("Blocked by protected canon TBD")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open Resolve protected TBD" })).toBeInTheDocument();
  });
});
