import { useCallback, useEffect, useRef, useState } from "react";
import type { WorkflowRunDetail, WorkflowRunRecord } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { getWorkflowRun, listWorkflowRuns } from "../workflows/api";
import { WorkflowRunView } from "../workflows/WorkflowRunView";
import { QaReviewControls } from "./QaReviewControls";
import * as api from "./api";
import type { QaCheckRecord, QaCheckStatus, QaReviewStatus, QaRunDetail, QaRunRecord } from "./types";

interface VideoQaPanelProps {
  projectRootPath: string;
  assetVersionId: string;
  versionLabel: string;
}

const TERMINAL_WORKFLOW_STATUSES = ["completed", "cancelled", "failed", "rejected"];

function isActiveWorkflow(detail: WorkflowRunDetail | null): boolean {
  return detail !== null && !TERMINAL_WORKFLOW_STATUSES.includes(detail.run.status);
}

function isActiveQa(run: QaRunRecord | undefined): boolean {
  return run?.status === "queued" || run?.status === "running";
}

function workflowTargetsVersion(record: WorkflowRunRecord, assetVersionId: string): boolean {
  if (record.operationId !== "asset.run_video_qa") return false;
  try {
    const input = JSON.parse(record.inputJson) as Record<string, unknown>;
    return input.assetVersionId === assetVersionId;
  } catch {
    return false;
  }
}

function effectiveCheckStatus(check: QaCheckRecord): QaCheckStatus {
  if (check.reviewStatus === "overridden_pass") return "pass";
  if (check.reviewStatus === "overridden_fail") return "fail";
  return check.status;
}

function findingLabel(check: QaCheckRecord): string {
  const requirement = check.requirement;
  if (
    requirement && typeof requirement === "object" && "label" in requirement
    && typeof requirement.label === "string"
  ) return requirement.label;
  return check.checkId
    .replace(/^[^:]+:/, "")
    .replace(/_/g, " ")
    .replace(/^./, (letter) => letter.toUpperCase());
}

function statusLabel(value: string): string {
  return value.replace(/_/g, " ").toUpperCase();
}

export function VideoQaPanel({ projectRootPath, assetVersionId, versionLabel }: VideoQaPanelProps) {
  const [runs, setRuns] = useState<QaRunRecord[]>([]);
  const [detail, setDetail] = useState<QaRunDetail | null>(null);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [workflow, setWorkflow] = useState<WorkflowRunDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [creating, setCreating] = useState(false);
  const [busyCheckId, setBusyCheckId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const creatingRef = useRef(false);

  const loadQa = useCallback(async (preferredRunId?: string | null) => {
    const history = await api.listQaRuns(projectRootPath, assetVersionId);
    setRuns(history);
    const nextId = preferredRunId && history.some((run) => run.id === preferredRunId)
      ? preferredRunId
      : history[0]?.id ?? null;
    setSelectedRunId(nextId);
    setDetail(nextId ? await api.getQaRun(projectRootPath, nextId) : null);
  }, [assetVersionId, projectRootPath]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setRuns([]);
    setDetail(null);
    setSelectedRunId(null);
    setWorkflow(null);

    Promise.all([api.listQaRuns(projectRootPath, assetVersionId), listWorkflowRuns(projectRootPath)])
      .then(async ([history, workflowRuns]) => {
        const latestWorkflow = workflowRuns
          .filter((record) => workflowTargetsVersion(record, assetVersionId))
          .sort((left, right) => right.createdAt.localeCompare(left.createdAt))[0];
        const [nextDetail, restoredWorkflow] = await Promise.all([
          history[0] ? api.getQaRun(projectRootPath, history[0].id) : Promise.resolve(null),
          latestWorkflow ? getWorkflowRun(projectRootPath, latestWorkflow.id) : Promise.resolve(null),
        ]);
        if (cancelled) return;
        setRuns(history);
        setSelectedRunId(history[0]?.id ?? null);
        setDetail(nextDetail);
        setWorkflow(restoredWorkflow);
      })
      .catch((reason: unknown) => {
        if (!cancelled) setError(describeError(reason));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => { cancelled = true; };
  }, [assetVersionId, projectRootPath]);

  async function runQa() {
    if (creatingRef.current || isActiveWorkflow(workflow) || isActiveQa(runs[0])) return;
    creatingRef.current = true;
    setCreating(true);
    setError(null);
    try {
      const created = await api.createVideoQaWorkflow(projectRootPath, assetVersionId);
      const next = await api.advanceQaWorkflow(projectRootPath, created.run.id);
      setWorkflow(next);
      if (TERMINAL_WORKFLOW_STATUSES.includes(next.run.status)) await loadQa();
    } catch (reason: unknown) {
      setError(describeError(reason));
    } finally {
      creatingRef.current = false;
      setCreating(false);
    }
  }

  async function approveAndRun() {
    if (!workflow || creatingRef.current || workflow.run.status !== "waiting_for_approval") return;
    creatingRef.current = true;
    setCreating(true);
    setError(null);
    try {
      const approved = await api.approveQaWorkflow(
        projectRootPath,
        workflow.run.id,
        "approve-video-qa",
        "Video QA evidence disclosure reviewed",
      );
      // Store the approval immediately: if advancing fails below, the panel
      // must reflect the true (already-approved) server state rather than
      // a stale "waiting_for_approval" that would re-submit the same
      // approval and hit "already decided" on retry.
      setWorkflow(approved);
      const next = await api.advanceQaWorkflow(projectRootPath, approved.run.id);
      setWorkflow(next);
      if (TERMINAL_WORKFLOW_STATUSES.includes(next.run.status)) await loadQa();
    } catch (reason: unknown) {
      setError(describeError(reason));
    } finally {
      creatingRef.current = false;
      setCreating(false);
    }
  }

  function handleWorkflowChange(next: WorkflowRunDetail) {
    setWorkflow(next);
    if (TERMINAL_WORKFLOW_STATUSES.includes(next.run.status)) {
      void loadQa(selectedRunId).catch((reason: unknown) => setError(describeError(reason)));
    }
  }

  async function review(checkId: string, reviewStatus: QaReviewStatus, note: string | null) {
    if (!detail) return;
    setBusyCheckId(checkId);
    setError(null);
    try {
      const next = await api.reviewQaCheck({
        projectRootPath,
        qaRunId: detail.run.id,
        checkId,
        reviewStatus,
        note,
      });
      setDetail(next);
      setRuns((history) => history.map((run) => run.id === next.run.id ? next.run : run));
    } catch (reason: unknown) {
      setError(describeError(reason));
    } finally {
      setBusyCheckId(null);
    }
  }

  const newestRun = runs[0];
  const overall = detail?.run.overallStatus ?? detail?.run.status ?? newestRun?.overallStatus ?? newestRun?.status ?? "not_run";
  const actionDisabled = loading || creating || isActiveWorkflow(workflow) || isActiveQa(newestRun);

  return (
    <section className="qa-panel video-qa-panel" aria-label={`Video QA for ${versionLabel}`}>
      <header className="qa-panel-header">
        <div>
          <h4>Video QA</h4>
          <p>Exact candidate {assetVersionId}</p>
        </div>
        <button type="button" disabled={actionDisabled} onClick={() => void runQa()}>
          Run Video QA
        </button>
      </header>

      {error ? <p role="alert">{error}</p> : null}
      {loading ? <p>Loading Video QA history…</p> : null}
      {!loading ? (
        <div className={`qa-overall qa-overall--${overall}`} data-testid="video-qa-effective-overall">
          <span>Effective overall</span>
          <strong>{statusLabel(overall)}</strong>
        </div>
      ) : null}

      {workflow?.run.status === "waiting_for_approval" ? (
        <section className="qa-execution-review" aria-label="Video QA execution review">
          <div>
            <strong>Review exact video evidence</strong>
            <p>Candidate {assetVersionId} and its declared immutable references will be evaluated only after approval.</p>
          </div>
          <button type="button" disabled={creating} onClick={() => void approveAndRun()}>
            Approve and Run Video QA
          </button>
        </section>
      ) : null}

      {workflow && isActiveWorkflow(workflow) && workflow.run.status !== "waiting_for_approval" ? (
        <WorkflowRunView projectRootPath={projectRootPath} detail={workflow} onChange={handleWorkflowChange} />
      ) : null}

      {detail?.run.errorMessage ? <p role="alert">{detail.run.errorMessage}</p> : null}
      {detail?.checks.length ? (
        <section aria-label="Video QA findings">
          <h5>Findings</h5>
          <ul>
            {detail.checks.map((check) => {
              const label = findingLabel(check);
              return (
                <li key={check.id} aria-label={`${label} QA finding`} className={`qa-check qa-check--${effectiveCheckStatus(check)}`}>
                  <div className="qa-check-copy">
                    <strong>{label}</strong>
                    <p>Evaluator finding: <b>{statusLabel(check.status)}</b></p>
                    <p>Human decision: <b>{statusLabel(check.reviewStatus)}</b>{check.reviewNote ? ` — ${check.reviewNote}` : ""}</p>
                    <p>Effective status: <b>{statusLabel(effectiveCheckStatus(check))}</b></p>
                    <p>{check.observed}</p>
                    <p>{check.reason}</p>
                    <QaReviewControls
                      label={label}
                      disabled={busyCheckId !== null}
                      initialNote={check.reviewNote}
                      onReview={(reviewStatus, note) => review(check.checkId, reviewStatus, note)}
                    />
                  </div>
                </li>
              );
            })}
          </ul>
        </section>
      ) : null}

      {!loading ? (
        <section className="qa-history" aria-label={`Video QA history for ${versionLabel}`}>
          <h5>Video QA History</h5>
          {runs.length === 0 ? <p>No Video QA history for this candidate.</p> : (
            <ul>
              {runs.map((run) => (
                <li key={run.id}>
                  <button
                    type="button"
                    aria-pressed={run.id === selectedRunId}
                    onClick={() => {
                      setSelectedRunId(run.id);
                      void api.getQaRun(projectRootPath, run.id).then(setDetail).catch((reason: unknown) => setError(describeError(reason)));
                    }}
                  >
                    <span>{run.id}</span>
                    <span>{statusLabel(run.overallStatus ?? run.status)}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>
      ) : null}
    </section>
  );
}
