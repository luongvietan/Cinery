import { type FormEvent, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  AppCommandError,
  ProjectSummary,
  RecentProject,
} from "@cinematic/domain";
import { createProject, listRecentProjects, openProject } from "./api";
import { RecentProjects } from "./RecentProjects";

interface ProjectHomeProps {
  onProjectOpened: (project: ProjectSummary) => void;
}

type PendingAction = "create" | "open" | null;

function isAppCommandError(value: unknown): value is AppCommandError {
  return (
    typeof value === "object" &&
    value !== null &&
    "message" in value &&
    typeof (value as { message: unknown }).message === "string"
  );
}

function describeError(error: unknown): string {
  if (isAppCommandError(error)) {
    return error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return "Something went wrong. Please try again.";
}

export function ProjectHome({ onProjectOpened }: ProjectHomeProps) {
  const [recentProjects, setRecentProjects] = useState<RecentProject[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [pendingAction, setPendingAction] = useState<PendingAction>(null);
  const [pendingRootPath, setPendingRootPath] = useState<string | null>(null);
  const [projectName, setProjectName] = useState("");

  useEffect(() => {
    let cancelled = false;

    listRecentProjects()
      .then((projects) => {
        if (!cancelled) {
          setRecentProjects(projects);
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setError(describeError(err));
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  async function pickDirectory(): Promise<string | null> {
    const selected = await open({ directory: true, multiple: false });
    return typeof selected === "string" ? selected : null;
  }

  async function handleCreateClick() {
    setError(null);
    const rootPath = await pickDirectory();
    if (!rootPath) {
      return;
    }
    setPendingRootPath(rootPath);
    setProjectName("");
  }

  function handleCreateCancel() {
    setPendingRootPath(null);
    setProjectName("");
  }

  async function handleCreateSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!pendingRootPath) {
      return;
    }

    setPendingAction("create");
    setError(null);
    try {
      const project = await createProject({
        rootPath: pendingRootPath,
        name: projectName,
      });
      setPendingRootPath(null);
      onProjectOpened(project);
    } catch (err) {
      setError(describeError(err));
    } finally {
      setPendingAction(null);
    }
  }

  async function handleOpenClick() {
    setError(null);
    const rootPath = await pickDirectory();
    if (!rootPath) {
      return;
    }
    await runOpen(rootPath);
  }

  async function handleOpenRecent(rootPath: string) {
    setError(null);
    await runOpen(rootPath);
  }

  async function runOpen(rootPath: string) {
    setPendingAction("open");
    try {
      const project = await openProject({ rootPath });
      onProjectOpened(project);
    } catch (err) {
      setError(describeError(err));
    } finally {
      setPendingAction(null);
    }
  }

  return (
    <main>
      <h1>AI Cinematic Production OS</h1>
      <p>Local-first cinematic project workspace.</p>

      {error && <p role="alert">{error}</p>}

      <div>
        <button
          type="button"
          onClick={handleCreateClick}
          disabled={pendingAction === "create" || pendingRootPath !== null}
        >
          Create Project
        </button>
        <button
          type="button"
          onClick={handleOpenClick}
          disabled={pendingAction === "open"}
        >
          Open Project
        </button>
      </div>

      {pendingRootPath && (
        <form onSubmit={handleCreateSubmit}>
          <label htmlFor="new-project-name">Project name</label>
          <input
            id="new-project-name"
            value={projectName}
            onChange={(event) => setProjectName(event.target.value)}
            required
          />
          <button type="submit" disabled={pendingAction === "create"}>
            Create
          </button>
          <button type="button" onClick={handleCreateCancel}>
            Cancel
          </button>
        </form>
      )}

      <RecentProjects projects={recentProjects} onOpen={handleOpenRecent} />
    </main>
  );
}
