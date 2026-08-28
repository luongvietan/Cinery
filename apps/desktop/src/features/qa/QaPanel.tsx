import { useEffect, useMemo, useState } from "react";
import type { WorkflowRunDetail } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import * as api from "./api";
import { effectiveCheckStatus, QaCheckList } from "./QaCheckList";
import { QaHistory } from "./QaHistory";
import type { QaReviewStatus, QaRunDetail, QaRunRecord } from "./types";

interface QaPanelProps {
  projectRootPath: string;
  assetVersionId: string;
  versionLabel: string;
  onRepair?: (qaRunId: string) => void;
}

interface ExecutionDisclosure {
  location: string;
  adapterId: string;
  modelId: string;
  referenceCount: number;
}

function disclosureFrom(workflow: WorkflowRunDetail): ExecutionDisclosure | null {
  const output = workflow.steps.find((step) => step.stepType === "compile_request")?.outputJson;
  if (!output) return null;
  try {
    const value = JSON.parse(output) as {
      executionLocation?: string;
      adapterId?: string;
      modelId?: string;
      request?: { references?: unknown[] };
    };
    return {
      location: value.executionLocation ?? (value.adapterId === "mock" ? "local" : "cloud:provider"),
      adapterId: value.adapterId ?? "unknown",
      modelId: value.modelId ?? "unknown",
      referenceCount: value.request?.references?.length ?? 0,
    };
  } catch {
    return null;
  }
}

function locationLabel(location: string): string {
  if (location === "local") return "LOCAL";
  if (location.startsWith("cloud:")) return `CLOUD: ${location.slice(6)}`;
  return location.toUpperCase();
}

export function QaPanel({
  projectRootPath,
  assetVersionId,
  versionLabel,
  onRepair,
}: QaPanelProps) {
  const [runs, setRuns] = useState<QaRunRecord[]>([]);
  const [detail, setDetail] = useState<QaRunDetail | null>(null);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [pendingWorkflow, setPendingWorkflow] = useState<WorkflowRunDetail | null>(null);
  const [pendingOperation, setPendingOperation] = useState<"qa" | "repair">("qa");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [busyCheckId, setBusyCheckId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function load(selectedId?: string | null) {
    const history = await api.listQaRuns(projectRootPath, assetVersionId);
    setRuns(history);
    const nextId = selectedId ?? history[0]?.id ?? null;
    setSelectedRunId(nextId);
    setDetail(nextId ? await api.getQaRun(projectRootPath, nextId) : null);
  }

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    Promise.resolve()
      .then(() => load())
      .catch((reason: unknown) => {
        if (!cancelled) setError(describeError(reason));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [projectRootPath, assetVersionId]);

  const groups = useMemo(() => {
    const checks = detail?.checks ?? [];
    return {
      failed: checks.filter((check) => effectiveCheckStatus(check) === "fail"),
      uncertain: checks.filter((check) => effectiveCheckStatus(check) === "uncertain"),
      passed: checks.filter((check) => effectiveCheckStatus(check) === "pass"),
    };
  }, [detail]);
  const disclosure = pendingWorkflow ? disclosureFrom(pendingWorkflow) : null;

  async function runQa() {
    setBusy(true);
    setError(null);
    try {
      const created = await api.createVisualQaWorkflow(projectRootPath, assetVersionId);
      setPendingOperation("qa");
      setPendingWorkflow(await api.advanceQaWorkflow(projectRootPath, created.run.id));
    } catch (reason: unknown) {
      setError(describeError(reason));
    } finally {
      setBusy(false);
    }
  }

  async function runRepair() {
    if (!detail) return;
    setBusy(true);
    setError(null);
    try {
      const providerId = detail.run.adapterId === "mock" ? "mock" : "openai";
      const modelId = providerId === "mock" ? "mock-image-v1" : "gpt-image-1";
      const created = await api.createVisualRepairWorkflow(
        projectRootPath,
        assetVersionId,
        detail.run.id,
        providerId,
        modelId,
      );
      setPendingOperation("repair");
      setPendingWorkflow(await api.advanceQaWorkflow(projectRootPath, created.run.id));
    } catch (reason: unknown) {
      setError(describeError(reason));
    } finally {
      setBusy(false);
    }
  }

  async function approveAndRun() {
    if (!pendingWorkflow) return;
    setBusy(true);
    setError(null);
    try {
      const approvalStep = pendingOperation === "repair" ? "approve-repair" : "approve-qa";
      await api.approveQaWorkflow(projectRootPath, pendingWorkflow.run.id, approvalStep);
      await api.advanceQaWorkflow(projectRootPath, pendingWorkflow.run.id);
      setPendingWorkflow(null);
      await load();
    } catch (reason: unknown) {
      setError(describeError(reason));
    } finally {
      setBusy(false);
    }
  }

  async function rejectQa() {
    if (!pendingWorkflow) return;
    setBusy(true);
    try {
      await api.rejectQaWorkflow(
        projectRootPath,
        pendingWorkflow.run.id,
        pendingOperation === "repair" ? "approve-repair" : "approve-qa",
      );
      setPendingWorkflow(null);
      await load();
    } catch (reason: unknown) {
      setError(describeError(reason));
    } finally {
      setBusy(false);
    }
  }

  async function review(
    checkId: string,
    reviewStatus: QaReviewStatus,
    note: string | null,
  ) {
    if (!detail) return;
    setBusyCheckId(checkId);
    setError(null);
    try {
      setDetail(
        await api.reviewQaCheck({
          projectRootPath,
          qaRunId: detail.run.id,
          checkId,
          reviewStatus,
          note,
        }),
      );
      await load(detail.run.id);
    } catch (reason: unknown) {
      setError(describeError(reason));
    } finally {
      setBusyCheckId(null);
    }
  }

  return (
    <section className="qa-panel" aria-label={`Visual QA for ${versionLabel}`}>
      <header className="qa-panel-header">
        <div>
          <h3>Visual QA</h3>
          <p>{versionLabel}, exact Asset Version evidence</p>
        </div>
        <button type="button" disabled={busy} onClick={() => void runQa()}>
          {busy ? "Preparing QA…" : "Run QA"}
        </button>
      </header>

      {busy ? (
        <p className="qa-busy-note" role="note" id="qa-busy-reason">
          A QA action is in progress; controls are paused until it finishes.
        </p>
      ) : null}

      {error ? <p role="alert">{error}</p> : null}
      {pendingWorkflow && disclosure ? (
        <section className="qa-execution-review" aria-label="QA execution review">
          <div>
            <strong>{locationLabel(disclosure.location)}</strong>
            <p>
              {disclosure.adapterId} · {disclosure.modelId} · candidate plus {disclosure.referenceCount} declared reference image(s)
            </p>
            {disclosure.location.startsWith("cloud:") ? (
              <p>Only this candidate and the listed canonical references leave the device.</p>
            ) : null}
          </div>
          <div>
            <button
              type="button"
              disabled={busy}
              aria-describedby={busy ? "qa-busy-reason" : undefined}
              onClick={() => void approveAndRun()}
            >
              Approve and Run QA
            </button>
            <button type="button" className="qa-secondary-button" disabled={busy} onClick={() => void rejectQa()}>
              Cancel QA
            </button>
          </div>
        </section>
      ) : null}

      {loading ? <p className="qa-loading">Loading QA history…</p> : null}
      {!loading && detail ? (
        <>
          <section className="qa-summary" aria-label="QA summary">
            <div className={`qa-overall qa-overall--${detail.run.overallStatus ?? detail.run.status}`}>
              <span>Overall</span>
              <strong>{(detail.run.overallStatus ?? detail.run.status).replace("_", " ").toUpperCase()}</strong>
            </div>
            <dl>
              <div><dt>Checks</dt><dd>{detail.checks.length}</dd></div>
              <div><dt>Passed</dt><dd>{groups.passed.length}</dd></div>
              <div><dt>Failed</dt><dd>{groups.failed.length}</dd></div>
              <div><dt>Needs review</dt><dd>{groups.uncertain.length}</dd></div>
            </dl>
          </section>
          <div className="qa-provenance">
            <dl>
              <div><dt>Provider</dt><dd>{detail.run.adapterId ?? "Unavailable"} · {detail.run.modelId ?? "Unavailable"}</dd></div>
              <div><dt>Location</dt><dd><span className="qa-location">{locationLabel(detail.run.executionLocation)}</span></dd></div>
              <div><dt>QA run</dt><dd>{detail.run.id}</dd></div>
              <div><dt>Asset Version</dt><dd>{detail.run.assetVersionId}</dd></div>
              <div><dt>References</dt><dd>{detail.run.checkPlan.referenceAssetVersionIds.join(", ") || "None"}</dd></div>
              <div><dt>Completed</dt><dd>{detail.run.completedAt ? new Date(detail.run.completedAt).toLocaleString() : "Pending"}</dd></div>
            </dl>
          </div>
          <QaCheckList title="Failed" ariaLabel="Failed checks" checks={groups.failed} busyCheckId={busyCheckId} onReview={review} />
          <QaCheckList title="Needs Review" ariaLabel="Checks needing review" checks={groups.uncertain} busyCheckId={busyCheckId} onReview={review} />
          <QaCheckList title="Passed" ariaLabel="Passed checks" checks={groups.passed} busyCheckId={busyCheckId} onReview={review} />
          {groups.failed.length > 0 ? (
            <button
              type="button"
              className="qa-repair-button"
              disabled={busy}
              onClick={() => {
                if (onRepair) onRepair(detail.run.id);
                else void runRepair();
              }}
            >
              Repair Failed Checks
            </button>
          ) : null}
        </>
      ) : null}
      {!loading ? (
        <QaHistory
          runs={runs}
          selectedRunId={selectedRunId}
          onSelect={(qaRunId) => {
            setSelectedRunId(qaRunId);
            void load(qaRunId).catch((reason: unknown) => setError(describeError(reason)));
          }}
        />
      ) : null}
    </section>
  );
}
