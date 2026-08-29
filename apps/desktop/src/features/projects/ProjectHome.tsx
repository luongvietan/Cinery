import { type FormEvent, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { ProjectSummary, RecentProject } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { createProject, listRecentProjects, openProject } from "./api";
import { RecentProjects } from "./RecentProjects";

interface ProjectHomeProps {
  onProjectOpened: (project: ProjectSummary) => void;
}

type PendingAction = "create" | "open" | null;

const FIRST_STEPS = [
  {
    title: "Create a project",
    detail: "One folder on your computer holds everything: story, characters, and scenes.",
  },
  {
    title: "Connect an AI service",
    detail: "Paste in the address and key of any image or video API you already use.",
  },
  {
    title: "Generate your first shot",
    detail: "Cinery keeps each character looking the same across every scene.",
  },
];

export function ProjectHome({ onProjectOpened }: ProjectHomeProps) {
  const [recentProjects, setRecentProjects] = useState<RecentProject[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [pendingAction, setPendingAction] = useState<PendingAction>(null);
  const [creating, setCreating] = useState(false);
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

  function handleCreateClick() {
    setError(null);
    setCreating(true);
    setProjectName("");
  }

  function handleCreateCancel() {
    setCreating(false);
    setProjectName("");
  }

  async function handleCreateSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    setPendingAction("create");
    setError(null);
    try {
      // No folder picking: the backend derives the project folder from the
      // project name, so the whole flow is name-only.
      const project = await createProject({
        name: projectName,
      });
      handleCreateCancel();
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
    <main className="home">
      <header className="home__hero">
        <h1>Make films with AI, without losing your characters.</h1>
        <p className="home__lede">
          Cinery is a workspace for AI filmmaking. It keeps your story, cast, and
          every generated image or video organized in one project, so your
          characters look the same from the first shot to the last.
        </p>
      </header>

      <ol className="home__steps" aria-label="How Cinery works">
        {FIRST_STEPS.map((step, index) => (
          <li key={step.title} className="home__step">
            <span className="home__step-number" aria-hidden="true">{index + 1}</span>
            <div>
              <strong>{step.title}</strong>
              <p>{step.detail}</p>
            </div>
          </li>
        ))}
      </ol>

      {error && <p role="alert">{error}</p>}

      <div className="home__actions">
        <button
          type="button"
          className="home__primary"
          onClick={handleCreateClick}
          disabled={pendingAction !== null}
        >
          Create a project
        </button>
        <button
          type="button"
          onClick={handleOpenClick}
          disabled={pendingAction !== null}
        >
          Open an existing project
        </button>
      </div>

      {creating && (
        <form className="home__create-form" onSubmit={handleCreateSubmit}>
          <h2>New project</h2>
          <label htmlFor="new-project-name">
            <span>Project name</span>
            <input
              id="new-project-name"
              value={projectName}
              onChange={(event) => setProjectName(event.target.value)}
              placeholder="e.g. Night Harbor"
              autoFocus
              required
            />
          </label>
          <p className="home__folder-hint">
            Saved as a normal folder on your computer, named after your
            project. It works offline and stays yours.
          </p>
          <div className="workflow-form-actions">
            <button type="submit" disabled={pendingAction === "create"}>
              {pendingAction === "create" ? "Creating…" : "Create project"}
            </button>
            <button
              type="button"
              onClick={handleCreateCancel}
              disabled={pendingAction === "create"}
            >
              Cancel
            </button>
          </div>
        </form>
      )}

      <RecentProjects
        projects={recentProjects}
        onOpen={handleOpenRecent}
        disabled={pendingAction !== null}
      />
    </main>
  );
}
