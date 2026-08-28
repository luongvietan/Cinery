import type { WorkflowRunDetail } from "@cinematic/domain";
import { useEffect, useRef, useState } from "react";
import { describeError } from "../../lib/errors";
import { advanceWorkflowRun, approveWorkflowStep, cancelWorkflowExecution, cancelWorkflowRun, rejectWorkflowStep, retryWorkflowExecution } from "./api";
import { humanizeWorkflowStatus } from "./format";

export function WorkflowRunView({ projectRootPath, detail, onChange }: { projectRootPath: string; detail: WorkflowRunDetail; onChange: (detail: WorkflowRunDetail) => void }) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirming, setConfirming] = useState<"reject" | "cancel" | null>(null);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const approval = detail.steps.find((step) => step.stepType === "approval");
  const request = detail.steps.find((step) => step.stepType === "compile_request")?.outputJson;
  const context = detail.run.contextSnapshotJson;
  const executionStep = detail.steps.find((step) => step.stepDefinitionId === "execute");
  const executionConfig = (() => {
    try { const input = JSON.parse(detail.run.inputJson) as Record<string, unknown>; return { provider: typeof input.providerId === "string" ? input.providerId : "dry_run", model: typeof input.modelId === "string" ? input.modelId : "dry-run-v1" }; }
    catch { return { provider: "dry_run", model: "dry-run-v1" }; }
  })();

  useEffect(() => { headingRef.current?.focus(); }, [detail.run.id, detail.run.status]);

  async function run(action: () => Promise<WorkflowRunDetail>) { setPending(true); setError(null); try { onChange(await action()); setConfirming(null); } catch (reason) { setError(describeError(reason)); } finally { setPending(false); } }

  return (
    <section className="workflow-run" aria-labelledby="workflow-run-title">
      <header className="workflow-run-header">
        <div><h2 ref={headingRef} tabIndex={-1} id="workflow-run-title">Workflow run</h2><p>{detail.run.skillId}@{detail.run.skillVersion} · {detail.run.operationId}</p></div>
        <span className={`workflow-status workflow-status--${detail.run.status}`}><span aria-hidden="true">●</span><span>{humanizeWorkflowStatus(detail.run.status)}</span></span>
      </header>
      {error ? <p role="alert">{error}</p> : null}
      <ol className="workflow-step-list">{detail.steps.map((step) => <li key={step.id}><span aria-hidden="true">{step.status === "completed" ? "✓" : step.status === "failed" ? "!" : "○"}</span><span>{step.stepDefinitionId}</span><span>{humanizeWorkflowStatus(step.status)}</span></li>)}</ol>
      <details open={detail.run.status === "waiting_for_approval"}><summary>Context snapshot</summary><pre>{context ? JSON.stringify(JSON.parse(context), null, 2) : "Not captured yet."}</pre></details>
      <details open={detail.run.status === "waiting_for_approval"}><summary>Compiled request and prompt</summary><pre>{request ? JSON.stringify(JSON.parse(request), null, 2) : "Not compiled yet."}</pre></details>
      {detail.run.status === "waiting_for_approval" && approval ? <div className="workflow-approval"><h3>Approval required</h3><p>Review the immutable context and compiled request above before making a decision.</p>{confirming === "reject" ? <div role="group" aria-label="Confirm workflow rejection"><button className="workflow-danger" type="button" disabled={pending} onClick={() => run(() => rejectWorkflowStep(projectRootPath, detail.run.id, approval.stepDefinitionId, null))}>Confirm rejection</button><button className="workflow-secondary-inline" type="button" disabled={pending} onClick={() => setConfirming(null)}>Keep workflow</button></div> : <div><button type="button" disabled={pending} onClick={() => run(() => approveWorkflowStep(projectRootPath, detail.run.id, approval.stepDefinitionId, null))}>Approve request</button><button className="workflow-danger" type="button" disabled={pending} onClick={() => setConfirming("reject")}>Reject request</button></div>}</div> : null}
      {detail.run.status === "ready_for_execution" ? <div className="workflow-execution"><h3>Ready for explicit execution</h3><p>Provider: <strong>{executionConfig.provider}</strong> · Model: <strong>{executionConfig.model}</strong></p><p>{executionConfig.provider === "dry_run" ? "DryRun has not executed." : "Production generation has not executed."}</p><button type="button" disabled={pending} onClick={() => run(() => advanceWorkflowRun(projectRootPath, detail.run.id))}>{executionConfig.provider === "dry_run" ? "Execute Dry Run" : "Execute provider"}</button></div> : null}
      {detail.run.status === "running" && executionStep ? <div className="workflow-execution"><h3>Generation in progress</h3><p>Provider execution can be cancelled locally; provider-side cancellation is attempted when supported.</p><button className="workflow-danger" type="button" disabled={pending} onClick={() => run(() => cancelWorkflowExecution(projectRootPath, detail.run.id, executionStep.stepDefinitionId))}>Cancel generation</button></div> : null}
      {detail.run.status === "failed" && executionStep ? <div className="workflow-execution"><h3>Generation failed</h3><p>{detail.run.failureMessage ?? "The provider execution failed."}</p><button type="button" disabled={pending} onClick={() => run(() => retryWorkflowExecution(projectRootPath, detail.run.id, executionStep.stepDefinitionId))}>Retry execution</button></div> : null}
      {!(["completed", "rejected", "cancelled", "failed"] as string[]).includes(detail.run.status) ? confirming === "cancel" ? <div className="workflow-cancel-confirm" role="group" aria-label="Confirm workflow cancellation"><p>Cancellation is terminal and skips remaining work.</p><button className="workflow-danger" type="button" disabled={pending} onClick={() => run(() => cancelWorkflowRun(projectRootPath, detail.run.id))}>Confirm cancellation</button><button className="workflow-secondary-inline" type="button" disabled={pending} onClick={() => setConfirming(null)}>Keep workflow</button></div> : <button className="workflow-secondary" type="button" disabled={pending} onClick={() => setConfirming("cancel")}>Cancel workflow</button> : null}
      <section className="workflow-events" aria-labelledby="workflow-events-title"><h3 id="workflow-events-title">Event history</h3><ol>{detail.events.map((event) => <li key={event.id}><span>{event.sequence}</span><span>{humanizeWorkflowStatus(event.eventType)}</span><time>{event.createdAt}</time></li>)}</ol></section>
      {detail.providerExecutions?.length ? <section className="workflow-events" aria-labelledby="provider-executions-title"><h3 id="provider-executions-title">Provider executions</h3><ol>{detail.providerExecutions.map((execution) => <li key={execution.id}><span>Attempt {execution.attemptNumber}</span><span>{execution.providerId} · {execution.modelId} · {humanizeWorkflowStatus(execution.status)}</span><span>{execution.providerJobId ?? "No remote job"}</span></li>)}</ol></section> : null}
    </section>
  );
}
