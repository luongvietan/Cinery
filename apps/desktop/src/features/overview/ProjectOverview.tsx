import { useEffect, useState } from "react";
import type { ProjectOverview as ProjectOverviewData } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { getProjectOverview } from "./api";

export function ProjectOverview({
  projectRootPath,
  onNavigate,
}: {
  projectRootPath: string;
  onNavigate: (destination: "canon" | "assets" | "production") => void;
}) {
  const [overview, setOverview] = useState<ProjectOverviewData | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    setOverview(null);
    setError(null);
    void getProjectOverview(projectRootPath)
      .then((value) => { if (current) setOverview(value); })
      .catch((reason) => { if (current) setError(describeError(reason)); });
    return () => { current = false; };
  }, [projectRootPath]);

  if (error) return <p role="alert">{error}</p>;
  if (!overview) return <p role="status">Loading project overview…</p>;

  const blocked = overview.readiness.status === "blocked";
  return (
    <section className="project-overview" aria-labelledby="production-progress-title">
      <header className="project-overview__header">
        <div>
          <span className="production-kicker">Production / Readiness</span>
          <h2 id="production-progress-title">Production Progress</h2>
          <p>{blocked ? "Blocked by protected canon TBD" : "Follow the next durable production step."}</p>
        </div>
        {overview.readiness.nextAction ? (
          <button type="button" onClick={() => onNavigate(overview.readiness.nextAction!.destination)}>
            Open {overview.readiness.nextAction.title}
          </button>
        ) : <span className="project-overview__complete">Production path complete</span>}
      </header>
      <ol className="project-overview__steps">
        {overview.readiness.steps.map((step) => (
          <li key={step.id} className={`project-overview__step project-overview__step--${step.status}`}>
            <span aria-hidden="true">{step.status === "complete" ? "✓" : step.status === "blocked" ? "!" : "○"}</span>
            <div><strong>{step.title}</strong><p>{step.detail}</p></div>
          </li>
        ))}
      </ol>
      <footer className="project-overview__health">
        <span>{overview.healthSummary.openProtectedTbdCount} protected TBD{overview.healthSummary.openProtectedTbdCount === 1 ? "" : "s"} open</span>
        <span>{overview.healthSummary.activeJobCount} active job{overview.healthSummary.activeJobCount === 1 ? "" : "s"}</span>
      </footer>
    </section>
  );
}
