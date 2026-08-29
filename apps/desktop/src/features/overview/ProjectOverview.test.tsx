import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ProjectOverview as ProjectOverviewData } from "@cinematic/domain";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProjectOverview } from "./ProjectOverview";
import { getProjectOverview } from "./api";
import { getProjectHealth } from "./healthApi";

vi.mock("./api", () => ({ getProjectOverview: vi.fn() }));
vi.mock("./healthApi", () => ({ getProjectHealth: vi.fn() }));

const overview: ProjectOverviewData = {
  readiness: {
    status: "pending",
    nextAction: { id: "story_canon", title: "Story Canon", destination: "canon", characterEntityId: null, sceneId: null },
    steps: [
      { id: "story_canon", title: "Story Canon", status: "pending", detail: "Create the story foundation before production.", action: { id: "story_canon", title: "Story Canon", destination: "canon", characterEntityId: null, sceneId: null } },
      { id: "face_lock", title: "Canonical Face", status: "complete", detail: "Every character has a canonical face reference.", action: null },
    ],
  },
  healthSummary: { openProtectedTbdCount: 0, openTbdCount: 0, activeJobCount: 0 },
  recentActivity: [],
  activeJobs: [],
  sceneReadiness: [],
};

describe("ProjectOverview", () => {
  beforeEach(() => {
    vi.mocked(getProjectOverview).mockReset().mockResolvedValue(overview);
    vi.mocked(getProjectHealth).mockReset().mockResolvedValue([]);
  });

  it("shows backend-derived progress and takes the next action to its existing workspace", async () => {
    const onNavigate = vi.fn();
    render(<ProjectOverview projectRootPath="/projects/red-door" onNavigate={onNavigate} />);

    expect(await screen.findByRole("heading", { name: "Production Progress" })).toBeInTheDocument();
    expect(screen.getByText("Create the story foundation before production.")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Continue with Story Canon" }));

    expect(onNavigate).toHaveBeenCalledWith({
      id: "story_canon",
      title: "Story Canon",
      destination: "canon",
      characterEntityId: null,
      sceneId: null,
    });
  });

  it("makes a protected TBD blocker explicit rather than presenting a ready action", async () => {
    vi.mocked(getProjectOverview).mockResolvedValue({
      ...overview,
      readiness: {
        ...overview.readiness,
        status: "blocked",
        nextAction: { id: "resolve_protected_tbd", title: "Resolve protected TBD", destination: "canon", characterEntityId: null, sceneId: "scene-001" },
        steps: [{ id: "cinema_compilation", title: "Cinema Compilation", status: "blocked", detail: "A protected open TBD blocks this scene from compilation.", action: { id: "resolve_protected_tbd", title: "Resolve protected TBD", destination: "canon", characterEntityId: null, sceneId: "scene-001" } }],
      },
      healthSummary: { openProtectedTbdCount: 1, openTbdCount: 1, activeJobCount: 0 },
    });

    render(<ProjectOverview projectRootPath="/projects/red-door" onNavigate={vi.fn()} />);

    expect(await screen.findByText("Blocked by protected canon TBD")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Continue with Resolve protected TBD" })).toBeInTheDocument();
  });

  it("uses each readiness card action rather than discarding its scope metadata", async () => {
    const onNavigate = vi.fn();
    vi.mocked(getProjectOverview).mockResolvedValue({
      ...overview,
      readiness: {
        ...overview.readiness,
        nextAction: null,
        steps: [{
          id: "cinema_compilation",
          title: "Cinema Compilation",
          status: "pending",
          detail: "Compile Scene 002.",
          action: {
            id: "cinema_compilation",
            title: "Cinema Compilation",
            destination: "scenes",
            characterEntityId: null,
            sceneId: "scene-002",
          },
        }],
      },
    });
    render(<ProjectOverview projectRootPath="/projects/red-door" onNavigate={onNavigate} />);

    await userEvent.click(await screen.findByRole("button", { name: "Open Cinema Compilation" }));

    expect(onNavigate).toHaveBeenCalledWith({
      id: "cinema_compilation",
      title: "Cinema Compilation",
      destination: "scenes",
      characterEntityId: null,
      sceneId: "scene-002",
    });
  });

  it("surfaces each backend-ranked scene rather than hiding later production work", async () => {
    vi.mocked(getProjectOverview).mockResolvedValue({
      ...overview,
      sceneReadiness: [
        { sceneId: "scene-002", title: "Scene 002", status: "pending", detail: "Needs compilation.", action: { id: "cinema_compilation", title: "Cinema Compilation", destination: "scenes", characterEntityId: null, sceneId: "scene-002" } },
        { sceneId: "scene-001", title: "Scene 001", status: "complete", detail: "Compiled.", action: null },
      ],
    });
    render(<ProjectOverview projectRootPath="/projects/red-door" onNavigate={vi.fn()} />);

    expect(await screen.findByRole("heading", { name: "Scene readiness" })).toBeInTheDocument();
    expect(screen.getByText("Scene 002")).toBeInTheDocument();
    expect(screen.getByText("Scene 001")).toBeInTheDocument();
  });
});
