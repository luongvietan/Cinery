import type { RecentProject } from "@cinematic/domain";

interface RecentProjectsProps {
  projects: RecentProject[];
  onOpen: (rootPath: string) => void;
  disabled?: boolean;
}

export function RecentProjects({
  projects,
  onOpen,
  disabled = false,
}: RecentProjectsProps) {
  return (
    <section aria-label="Recent Projects">
      <h2>Recent Projects</h2>
      {projects.length === 0 ? (
        <p>No recent projects yet.</p>
      ) : (
        <ul>
          {projects.map((project) => (
            <li key={project.projectId}>
              <button
                type="button"
                onClick={() => onOpen(project.rootPath)}
                disabled={disabled}
              >
                {project.name}
              </button>
              <span>{project.rootPath}</span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
