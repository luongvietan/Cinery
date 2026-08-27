import type { ProjectSummary } from "@cinematic/domain";

interface ProjectWorkspaceProps {
  project: ProjectSummary;
}

export function ProjectWorkspace({ project }: ProjectWorkspaceProps) {
  return (
    <>
      <header>
        <h1>{project.name}</h1>
        <span>{project.rootPath}</span>
      </header>
      <nav>
        <button type="button">Assets</button>
      </nav>
      <section aria-label="Project workspace" />
    </>
  );
}
