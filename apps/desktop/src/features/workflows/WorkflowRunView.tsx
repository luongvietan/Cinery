import type { WorkflowRunDetail } from "@cinematic/domain";
import { useEffect, useRef, useState } from "react";
import { describeError } from "../../lib/errors";
import { openPanel } from "../../lib/panelNavigation";
import { advanceWorkflowRun, approveWorkflowStep, cancelWorkflowExecution, cancelWorkflowRun, getWorkflowRun, rejectWorkflowStep, retryWorkflowExecution } from "./api";
import { operationLabel, runStatusLabel, stepLabel, stepStatusLabel } from "./labels";

/** How often an in-progress run re-reads authoritative state from SQLite. */
const RUN_REFRESH_MS = 1500;

const TERMINAL_STATUSES = ["completed", "rejected", "cancelled", "failed"];

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
    try { const input = JSON.parse(detail.run.inputJson) as Record<string, unknown>; return { provider: typeof input.providerId === "string" && input.providerId ? input.providerId : null, model: typeof input.modelId === "string" ? input.modelId : null }; }
    catch { return { provider: null, model: null }; }
  })();
  const isTestRun = executionConfig.provider === null;
  const active = !TERMINAL_STATUSES.includes(detail.run.status);
  // The most recent provider execution for this run: provider, model, job,
  // and progress come from durable state, never in-memory runner state.
  const latestExecution = detail.providerExecutions?.[detail.providerExecutions.length - 1] ?? null;
  const backgroundRunning = detail.run.status === "running" && latestExecution !== null &&
    !["succeeded", "failed", "cancelled"].includes(latestExecution.status);

  useEffect(() => { headingRef.current?.focus(); }, [detail.run.id, detail.run.status]);

  // P10.1 background refresh: while the run is non-terminal, poll
  // authoritative state. When the background runner completes the job, the
  // UI picks it up even though the originating invoke already returned.
  useEffect(() => {
    if (!active) return;
    const timer = window.setInterval(() => {
      getWorkflowRun(projectRootPath, detail.run.id)
        .then((next) => {
          setError(null);
          onChange(next);
        })
        .catch(() => {
          /* transient read failures keep the last known state */
        });
    }, RUN_REFRESH_MS);
    return () => window.clearInterval(timer);
  }, [projectRootPath, detail.run.id, active, onChange]);

  async function run(action: () => Promise<WorkflowRunDetail>) { setPending(true); setError(null); try { onChange(await action()); setConfirming(null); } catch (reason) { setError(describeError(reason)); } finally { setPending(false); } }

  function prettyJson(value: string | null | undefined): string {
    if (!value) return "Not captured yet.";
    try { return JSON.stringify(JSON.parse(value), null, 2); } catch { return value; }
  }

  return (
    <section className="workflow-run" aria-labelledby="workflow-run-title">
      <header className="workflow-run-header">
        <div>
          <h2 ref={headingRef} tabIndex={-1} id="workflow-run-title">{operationLabel(detail.run.operationId)}</h2>
          <p>Started {detail.run.createdAt ? new Date(detail.run.createdAt).toLocaleString() : ""}</p>
        </div>
        <span className={`workflow-status workflow-status--${detail.run.status}`}><span aria-hidden="true">●</span><span>{runStatusLabel(detail.run.status)}</span></span>
      </header>
      {error ? <p role="alert">{error}</p> : null}
      <ol className="workflow-step-list" aria-label="Steps">{detail.steps.map((step) => <li key={step.id}><span aria-hidden="true">{step.status === "completed" ? "✓" : step.status === "failed" ? "!" : "○"}</span><span>{stepLabel(step.stepDefinitionId)}</span><span>{stepStatusLabel(step.status)}</span></li>)}</ol>
      {detail.run.status === "waiting_for_approval" && approval ? (
        <div className="workflow-approval">
          <h3>Check before Cinery generates</h3>
          <p>Look over what the AI will use below. Approve to start the generation, or stop here — nothing has been sent yet.</p>
          {confirming === "reject" ? <div role="group" aria-label="Confirm stopping this generation"><p>Stop this generation? It can't be resumed.</p><button className="workflow-danger" type="button" disabled={pending} onClick={() => run(() => rejectWorkflowStep(projectRootPath, detail.run.id, approval.stepDefinitionId, null))}>Yes, stop it</button><button className="workflow-secondary-inline" type="button" disabled={pending} onClick={() => setConfirming(null)}>Keep going</button></div> : (
            <div>
              <button type="button" disabled={pending} onClick={() => run(() => approveWorkflowStep(projectRootPath, detail.run.id, approval.stepDefinitionId, null))}>Approve and generate</button>
              <button className="workflow-danger" type="button" disabled={pending} onClick={() => setConfirming("reject")}>Stop</button>
            </div>
          )}
        </div>
      ) : null}
      {detail.run.status === "ready_for_execution" ? (
        <div className="workflow-execution">
          <h3>Ready to generate</h3>
          <p>{isTestRun ? "No AI service is selected, so this will be a test run — it records the request without calling an AI." : `Using ${executionConfig.provider}${executionConfig.model ? ` · ${executionConfig.model}` : ""}.`}</p>
          {isTestRun ? <p>Want the real thing? <button type="button" className="workflow-link" onClick={() => openPanel("providers")}>Connect an AI service</button></p> : null}
          <button type="button" disabled={pending} onClick={() => run(() => advanceWorkflowRun(projectRootPath, detail.run.id))}>{isTestRun ? "Run test" : "Generate now"}</button>
        </div>
      ) : null}
      {detail.run.status === "running" && executionStep ? (
        <div className="workflow-execution" aria-live="polite">
          <h3>Generating…</h3>
          <p>
            {backgroundRunning && latestExecution
              ? `${latestExecution.providerId}${latestExecution.modelId ? ` · ${latestExecution.modelId}` : ""} is working in the background. You can leave this page — the generation keeps running, and the result appears here when it's done.`
              : "The AI is working. You can cancel — it stops here and nothing is saved."}
          </p>
          {backgroundRunning && latestExecution ? (
            <p>
              <span>Attempt {latestExecution.attemptNumber}</span>
              {latestExecution.status === "running" ? <span> · running</span> : null}
              {latestExecution.providerJobId ? <span> · job {latestExecution.providerJobId}</span> : null}
            </p>
          ) : null}
          <button className="workflow-danger" type="button" disabled={pending} onClick={() => run(() => cancelWorkflowExecution(projectRootPath, detail.run.id, executionStep.stepDefinitionId))}>Cancel generation</button>
        </div>
      ) : null}
      {detail.run.status === "failed" && executionStep ? (
        <div className="workflow-execution">
          <h3>Generation failed</h3>
          <p>{detail.run.failureMessage ?? "The AI service reported a problem."}</p>
          <div>
            <button type="button" disabled={pending} onClick={() => run(() => retryWorkflowExecution(projectRootPath, detail.run.id, executionStep.stepDefinitionId))}>Try again</button>
            <button type="button" className="workflow-secondary-inline" onClick={() => openPanel("providers")}>Check AI services</button>
          </div>
        </div>
      ) : null}
      {active && detail.run.status !== "waiting_for_approval" ? confirming === "cancel" ? (
        <div className="workflow-cancel-confirm" role="group" aria-label="Confirm cancelling this generation">
          <p>Cancel this generation? The work so far is kept, but it won't continue.</p>
          <button className="workflow-danger" type="button" disabled={pending} onClick={() => run(() => cancelWorkflowRun(projectRootPath, detail.run.id))}>Yes, cancel it</button>
          <button className="workflow-secondary-inline" type="button" disabled={pending} onClick={() => setConfirming(null)}>Keep going</button>
        </div>
      ) : <button className="workflow-secondary" type="button" disabled={pending} onClick={() => setConfirming("cancel")}>Cancel generation</button> : null}
      <details className="workflow-technical">
        <summary>Technical details</summary>
        <dl>
          <div><dt>Operation</dt><dd><code>{detail.run.skillId}@{detail.run.skillVersion} · {detail.run.operationId}</code></dd></div>
          <div><dt>Run id</dt><dd><code>{detail.run.id}</code></dd></div>
          {detail.run.failureCode ? <div><dt>Failure code</dt><dd><code>{detail.run.failureCode}</code></dd></div> : null}
        </dl>
        <details><summary>What the AI will use</summary><pre>{prettyJson(context)}</pre></details>
        <details><summary>Prompt sent to the AI</summary><pre>{prettyJson(request)}</pre></details>
        <section aria-labelledby="workflow-events-title"><h3 id="workflow-events-title">Event history</h3><ol>{detail.events.map((event) => <li key={event.id}><span>{event.sequence}</span><span>{event.eventType}</span><time>{event.createdAt}</time></li>)}</ol></section>
        {detail.providerExecutions?.length ? <section aria-labelledby="provider-executions-title"><h3 id="provider-executions-title">Provider executions</h3><ol>{detail.providerExecutions.map((execution) => <li key={execution.id}><span>Attempt {execution.attemptNumber}</span><span>{execution.providerId} · {execution.modelId} · {execution.status}</span><span>{execution.providerJobId ?? "No remote job"}</span></li>)}</ol></section> : null}
      </details>
    </section>
  );
}
