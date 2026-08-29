import { useState } from "react";
import type { ProjectSummary } from "@cinematic/domain";
import { ThemeToggle } from "./components/ThemeToggle";
import { ProjectHome } from "./features/projects/ProjectHome";
import { ProjectWorkspace } from "./features/projects/ProjectWorkspace";
import "./styles/app.css";

export default function App() {
  const [project, setProject] = useState<ProjectSummary | null>(null);

  if (project) {
    return (
      <>
        <div className="theme-toggle-slot">
          <ThemeToggle />
        </div>
        <ProjectWorkspace
          project={project}
          onCloseProject={() => setProject(null)}
        />
      </>
    );
  }

  return (
    <>
      <div className="theme-toggle-slot">
        <ThemeToggle />
      </div>
      <ProjectHome onProjectOpened={setProject} />
    </>
  );
}
