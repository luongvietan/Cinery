import { useState } from "react";
import type { ProjectSummary } from "@cinematic/domain";
import { ProjectHome } from "./features/projects/ProjectHome";
import { ProjectWorkspace } from "./features/projects/ProjectWorkspace";
import "./styles/app.css";

export default function App() {
  const [project, setProject] = useState<ProjectSummary | null>(null);

  if (project) {
    return (
      <ProjectWorkspace
        project={project}
        onCloseProject={() => setProject(null)}
      />
    );
  }

  return <ProjectHome onProjectOpened={setProject} />;
}
