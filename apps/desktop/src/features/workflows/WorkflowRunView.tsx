import type { WorkflowRunDetail } from "@cinematic/domain";
import { useState } from "react";
import { describeError } from "../../lib/errors";
import { advanceWorkflowRun, approveWorkflowStep, cancelWorkflowRun, rejectWorkflowStep } from "./api";
import { humanizeWorkflowStatus } from "./format";

export function WorkflowRunView({ projectRootPath, detail, onChange }: { projectRootPath: string; detail: WorkflowRunDetail; onChange: (detail: WorkflowRunDetail) => void }) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const approval = detail.steps.find((step) => step.stepType === "approval");
  const request = detail.steps.find((step) => step.stepType === "compile_request")?.outputJson;
  const context = detail.run.contextSnapshotJson;

  async function run(action: () => Promise<WorkflowRunDetail>) { setPending(true); setError(null); try { onChange(await action()); } catch (reason) { setError(describeError(reason)); } finally { setPending(false); } }

  return (
    <section className="workflow-run" aria-labelledby="workflow-run-title">
      <header className="workflow-run-header">
        <div><h2 id="workflow-run-title">Workflow run</h2><p>{detail.run.skillId}@{detail.run.skillVersion} · {detail.run.operationId}</p></div>
        <span className={`workflow-status workflow-status--${detail.run.status}`}><span aria-hidden="true">●</span><span>{humanizeWorkflowStatus(detail.run.status)}</span></span>
      </header>
      {error ? <p role="alert">{error}</p> : null}
      <ol className="workflow-step-list">{detail.steps.map((step) => <li key={step.id}><span aria-hidden="true">{step.status === "completed" ? "✓" : step.status === "failed" ? "!" : "○"}</span><span>{step.stepDefinitionId}</span><span>{humanizeWorkflowStatus(step.status)}</span></li>)}</ol>
      {detail.run.status === "waiting_for_approval" && approval ? <div className="workflow-approval"><h3>Approval required</h3><p>Review the immutable context and compiled request before making a decision.</p><div><button type="button" disabled={pending} onClick={() => run(() => approveWorkflowStep(projectRootPath, detail.run.id, approval.stepDefinitionId, null))}>Approve request</button><button className="workflow-danger" type="button" disabled={pending} onClick={() => run(() => rejectWorkflowStep(projectRootPath, detail.run.id, approval.stepDefinitionId, null))}>Reject request</button></div></div> : null}
      {detail.run.status === "ready_for_execution" ? <div className="workflow-execution"><h3>Ready for explicit execution</h3><p>Approval is recorded. DryRun has not executed.</p><button type="button" disabled={pending} onClick={() => run(() => advanceWorkflowRun(projectRootPath, detail.run.id))}>Execute Dry Run</button></div> : null}
      {!(["completed", "rejected", "cancelled", "failed"] as string[]).includes(detail.run.status) ? <button className="workflow-secondary" type="button" disabled={pending} onClick={() => run(() => cancelWorkflowRun(projectRootPath, detail.run.id))}>Cancel workflow</button> : null}
      <details><summary>Context snapshot</summary><pre>{context ? JSON.stringify(JSON.parse(context), null, 2) : "Not captured yet."}</pre></details>
      <details><summary>Compiled request and prompt</summary><pre>{request ? JSON.stringify(JSON.parse(request), null, 2) : "Not compiled yet."}</pre></details>
      <section className="workflow-events" aria-labelledby="workflow-events-title"><h3 id="workflow-events-title">Event history</h3><ol>{detail.events.map((event) => <li key={event.id}><span>{event.sequence}</span><span>{humanizeWorkflowStatus(event.eventType)}</span><time>{event.createdAt}</time></li>)}</ol></section>
    </section>
  );
}
