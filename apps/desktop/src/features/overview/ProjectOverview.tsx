import { useEffect, useState } from "react";
import type { OverviewAction, ProjectOverview as ProjectOverviewData, ReadinessStep } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { openPanel } from "../../lib/panelNavigation";
import { countConnectedAiServices, getProjectOverview } from "./api";
import { getProjectHealth } from "./healthApi";
import { readinessCopy } from "./readinessCopy";
import { AiDirectorBar } from "./AiDirectorBar";

function stepCopy(step: ReadinessStep): { title: string; detail: string; actionLabel: string | null } {
  const plain = readinessCopy(step.id);
  // Scene-specific backend detail (e.g. "Compile Scene 002.") carries scope the
  // generic copy can't, so it wins over the plain-language template.
  const detail = /scene \d+/i.test(step.detail) ? step.detail : plain?.detail ?? step.detail;
  const actionId = step.action?.id;
  const actionCopy = actionId ? readinessCopy(actionId) : null;
  return {
    title: plain?.title ?? step.title,
    detail,
    actionLabel: actionCopy?.actionLabel ?? plain?.actionLabel ?? (step.action ? `Open ${step.action.title}` : null),
  };
}

export function ProjectOverview({
  projectRootPath,
  onNavigate,
}: {
  projectRootPath: string;
  onNavigate: (action: OverviewAction) => void;
}) {
  const [overview, setOverview] = useState<ProjectOverviewData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [healthIssues, setHealthIssues] = useState<import("@cinematic/domain").ProjectHealthIssue[]>([]);
  const [aiServiceCount, setAiServiceCount] = useState<number | null>(null);

  useEffect(() => {
    let current = true;
    setOverview(null);
    setError(null);
    void Promise.all([getProjectOverview(projectRootPath), getProjectHealth(projectRootPath)])
      .then(([value, issues]) => { if (current) { setOverview(value); setHealthIssues(issues); } })
      .catch((reason) => { if (current) setError(describeError(reason)); });
    // A missing AI service is the hidden prerequisite of every generation
    // step, so surface it here rather than waiting for the first failure.
    countConnectedAiServices(projectRootPath)
      .then((count) => { if (current) setAiServiceCount(count); })
      .catch(() => { if (current) setAiServiceCount(null); });
    return () => { current = false; };
  }, [projectRootPath]);

  if (error) return <p role="alert">{error}</p>;
  if (!overview) return <p role="status">Loading project overview…</p>;

  const blocked = overview.readiness.status === "blocked";
  const nextAction = overview.readiness.nextAction;
  const nextCopy = nextAction ? readinessCopy(nextAction.id) : null;

  return (
    <section className="project-overview" aria-labelledby="production-progress-title">
      {aiServiceCount === 0 ? (
        <div className="provider-banner" role="status">
          <div>
            <strong>No AI service connected yet.</strong>
            <p>Cinery generates images and video through an AI service you connect. Connect one to unlock the steps below — your project and story stay on your computer either way.</p>
          </div>
          <button type="button" onClick={() => openPanel("providers")}>Connect an AI service</button>
        </div>
      ) : null}
      <header className="project-overview__header">
        <div>
          <span className="production-kicker">Production / Readiness</span>
          <h2 id="production-progress-title">Your path to the first shot</h2>
          <p>
            {blocked
              ? "One open question in your story needs an answer before the next step can run."
              : "Work top to bottom. Each step locks in a piece of your film so later scenes stay consistent."}
          </p>
        </div>
        {nextAction ? (
          <button type="button" className="home__primary" onClick={() => onNavigate(nextAction)}>
            {nextCopy ? `Continue: ${nextCopy.title}` : `Continue with ${nextAction.title}`}
          </button>
        ) : <span className="project-overview__complete">Production path complete</span>}
      </header>
      <AiDirectorBar projectRootPath={projectRootPath} />
      <ol className="project-overview__steps">
        {overview.readiness.steps.map((step) => {
          const copy = stepCopy(step);
          return (
            <li key={step.id} className={`project-overview__step project-overview__step--${step.status}`}>
              <span aria-hidden="true">{step.status === "complete" ? "✓" : step.status === "blocked" ? "!" : "○"}</span>
              <div>
                <strong>{copy.title}</strong>
                <p>{copy.detail}</p>
                {step.status !== "complete" && step.action ? <button type="button" onClick={() => onNavigate(step.action!)}>{copy.actionLabel}</button> : null}
              </div>
            </li>
          );
        })}
      </ol>
      <section className="project-overview__health-panel" aria-labelledby="project-health-title">
        <h3 id="project-health-title">Project Health</h3>
        <p>{healthIssues.filter((issue) => issue.severity === "error" || issue.severity === "fatal").length} errors · {healthIssues.filter((issue) => issue.severity === "warning").length} warnings</p>
        {healthIssues.length ? <ul>{healthIssues.map((issue) => <li key={`${issue.code}:${issue.entityId ?? "project"}`}><strong>{issue.code}</strong><span>{issue.message}</span>{issue.remediation ? <p>{issue.remediation}</p> : null}</li>)}</ul> : <p role="status">No integrity issues detected.</p>}
      </section>
      {overview.sceneReadiness.length ? <section className="project-overview__scenes" aria-labelledby="scene-readiness-title"><h3 id="scene-readiness-title">Scene readiness</h3><ol className="project-overview__steps">{overview.sceneReadiness.map((scene) => <li key={scene.sceneId} className={`project-overview__step project-overview__step--${scene.status}`}><span aria-hidden="true">{scene.status === "complete" ? "✓" : scene.status === "blocked" ? "!" : "○"}</span><div><strong>{scene.title}</strong><p>{scene.detail}</p>{scene.action ? <button type="button" onClick={() => onNavigate(scene.action!)}>Open {scene.action.title}</button> : null}</div></li>)}</ol></section> : null}
      <footer className="project-overview__health">
        <span>{overview.healthSummary.openProtectedTbdCount} protected TBD{overview.healthSummary.openProtectedTbdCount === 1 ? "" : "s"} open</span>
        <span>{overview.healthSummary.activeJobCount} active job{overview.healthSummary.activeJobCount === 1 ? "" : "s"}</span>
      </footer>
    </section>
  );
}
