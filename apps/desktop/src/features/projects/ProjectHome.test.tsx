import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { open } from "@tauri-apps/plugin-dialog";
import type { ProjectSummary } from "@cinematic/domain";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProjectHome } from "./ProjectHome";
import { createProject, listRecentProjects, openProject } from "./api";

vi.mock("./api");
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

describe("ProjectHome", () => {
  beforeEach(() => {
    vi.mocked(listRecentProjects).mockReset().mockResolvedValue([]);
    vi.mocked(createProject).mockReset();
    vi.mocked(openProject).mockReset();
    vi.mocked(open).mockReset();
  });

  it("shows create and open actions", async () => {
    render(<ProjectHome onProjectOpened={vi.fn()} />);
    expect(
      screen.getByRole("button", { name: "Create Project" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Open Project" }),
    ).toBeInTheDocument();
  });

  it("shows an empty state when there are no recent projects", async () => {
    render(<ProjectHome onProjectOpened={vi.fn()} />);
    expect(
      await screen.findByText(/no recent projects/i),
    ).toBeInTheDocument();
  });

  it("opens a recent project into the workspace", async () => {
    vi.mocked(listRecentProjects).mockResolvedValue([
      {
        projectId: "01JRECENT",
        rootPath: "/projects/red-door",
        name: "Red Door",
        lastOpenedAt: "2026-08-27T06:00:00Z",
      },
    ]);

    vi.mocked(openProject).mockResolvedValue({
      id: "01JRECENT",
      rootPath: "/projects/red-door",
      name: "Red Door",
      schemaVersion: 1,
      createdAt: "2026-08-27T05:00:00Z",
      updatedAt: "2026-08-27T05:00:00Z",
    });

    const onProjectOpened = vi.fn();
    render(<ProjectHome onProjectOpened={onProjectOpened} />);
    expect(await screen.findByText("Red Door")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Red Door" }));

    await waitFor(() => {
      expect(openProject).toHaveBeenCalledWith({
        rootPath: "/projects/red-door",
      });
    });
    await waitFor(() => {
      expect(onProjectOpened).toHaveBeenCalledWith(
        expect.objectContaining({ id: "01JRECENT" }),
      );
    });
  });

  it("creates a project after picking a directory and naming it", async () => {
    vi.mocked(open).mockResolvedValue("/projects/new-film");
    vi.mocked(createProject).mockResolvedValue({
      id: "01JNEW",
      rootPath: "/projects/new-film",
      name: "New Film",
      schemaVersion: 1,
      createdAt: "2026-08-27T05:00:00Z",
      updatedAt: "2026-08-27T05:00:00Z",
    });

    const onProjectOpened = vi.fn();
    const user = userEvent.setup();
    render(<ProjectHome onProjectOpened={onProjectOpened} />);

    await user.click(screen.getByRole("button", { name: "Create Project" }));
    const nameInput = await screen.findByLabelText(/project name/i);
    await user.type(nameInput, "New Film");
    await user.click(screen.getByRole("button", { name: "Create" }));

    await waitFor(() => {
      expect(createProject).toHaveBeenCalledWith({
        rootPath: "/projects/new-film",
        name: "New Film",
      });
    });
    await waitFor(() => {
      expect(onProjectOpened).toHaveBeenCalledWith(
        expect.objectContaining({ id: "01JNEW" }),
      );
    });
  });

  it("does nothing when the user cancels the directory picker", async () => {
    vi.mocked(open).mockResolvedValue(null);
    render(<ProjectHome onProjectOpened={vi.fn()} />);

    await userEvent.click(
      screen.getByRole("button", { name: "Open Project" }),
    );

    await waitFor(() => {
      expect(open).toHaveBeenCalled();
    });
    expect(openProject).not.toHaveBeenCalled();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("shows the command error message when opening a project fails", async () => {
    vi.mocked(open).mockResolvedValue("/projects/broken");
    vi.mocked(openProject).mockRejectedValue({
      code: "PROJECT_NOT_FOUND",
      message: "No project found at the selected path.",
    });

    render(<ProjectHome onProjectOpened={vi.fn()} />);
    await userEvent.click(
      screen.getByRole("button", { name: "Open Project" }),
    );

    expect(
      await screen.findByText("No project found at the selected path."),
    ).toBeInTheDocument();
  });

  it("disables the open action while the command is in flight", async () => {
    vi.mocked(open).mockResolvedValue("/projects/slow");
    let resolveOpen!: (value: ProjectSummary) => void;
    vi.mocked(openProject).mockReturnValue(
      new Promise<ProjectSummary>((resolve) => {
        resolveOpen = resolve;
      }),
    );

    render(<ProjectHome onProjectOpened={vi.fn()} />);
    await userEvent.click(
      screen.getByRole("button", { name: "Open Project" }),
    );

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: "Open Project" }),
      ).toBeDisabled();
    });

    resolveOpen({
      id: "01JSLOW",
      rootPath: "/projects/slow",
      name: "Slow",
      schemaVersion: 1,
      createdAt: "2026-08-27T05:00:00Z",
      updatedAt: "2026-08-27T05:00:00Z",
    });

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: "Open Project" }),
      ).not.toBeDisabled();
    });
  });

  it("disables cancel once a create is submitted, so a stale resolution can't be mistaken for a cancelled create", async () => {
    vi.mocked(open).mockResolvedValue("/projects/new-film");
    let resolveCreate!: (value: ProjectSummary) => void;
    vi.mocked(createProject).mockReturnValue(
      new Promise<ProjectSummary>((resolve) => {
        resolveCreate = resolve;
      }),
    );

    const onProjectOpened = vi.fn();
    const user = userEvent.setup();
    render(<ProjectHome onProjectOpened={onProjectOpened} />);

    await user.click(screen.getByRole("button", { name: "Create Project" }));
    const nameInput = await screen.findByLabelText(/project name/i);
    await user.type(nameInput, "New Film");
    await user.click(screen.getByRole("button", { name: "Create" }));

    const cancelButton = screen.getByRole("button", { name: "Cancel" });
    await waitFor(() => {
      expect(cancelButton).toBeDisabled();
    });

    // A disabled button cannot be clicked away by the user, so the in-flight
    // create can't be silently abandoned mid-flight.
    await user.click(cancelButton);
    expect(onProjectOpened).not.toHaveBeenCalled();

    resolveCreate({
      id: "01JNEW",
      rootPath: "/projects/new-film",
      name: "New Film",
      schemaVersion: 1,
      createdAt: "2026-08-27T05:00:00Z",
      updatedAt: "2026-08-27T05:00:00Z",
    });

    await waitFor(() => {
      expect(onProjectOpened).toHaveBeenCalledTimes(1);
    });
  });

  it("disables recent-project buttons while an open is in flight, preventing a second click from racing", async () => {
    vi.mocked(listRecentProjects).mockResolvedValue([
      {
        projectId: "01JA",
        rootPath: "/projects/a",
        name: "Project A",
        lastOpenedAt: "2026-08-27T06:00:00Z",
      },
      {
        projectId: "01JB",
        rootPath: "/projects/b",
        name: "Project B",
        lastOpenedAt: "2026-08-27T05:00:00Z",
      },
    ]);

    let resolveOpen!: (value: ProjectSummary) => void;
    vi.mocked(openProject).mockReturnValue(
      new Promise<ProjectSummary>((resolve) => {
        resolveOpen = resolve;
      }),
    );

    const onProjectOpened = vi.fn();
    render(<ProjectHome onProjectOpened={onProjectOpened} />);

    const buttonA = await screen.findByRole("button", { name: "Project A" });
    const buttonB = screen.getByRole("button", { name: "Project B" });

    await userEvent.click(buttonA);

    await waitFor(() => {
      expect(buttonA).toBeDisabled();
      expect(buttonB).toBeDisabled();
    });

    // A second click on either row must not fire while the first is pending.
    await userEvent.click(buttonB);
    expect(openProject).toHaveBeenCalledTimes(1);
    expect(openProject).toHaveBeenCalledWith({ rootPath: "/projects/a" });

    resolveOpen({
      id: "01JA",
      rootPath: "/projects/a",
      name: "Project A",
      schemaVersion: 1,
      createdAt: "2026-08-27T05:00:00Z",
      updatedAt: "2026-08-27T05:00:00Z",
    });

    await waitFor(() => {
      expect(onProjectOpened).toHaveBeenCalledTimes(1);
    });
    expect(onProjectOpened).toHaveBeenCalledWith(
      expect.objectContaining({ id: "01JA" }),
    );
  });
});
