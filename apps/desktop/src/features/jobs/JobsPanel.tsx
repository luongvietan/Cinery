import React, { useCallback, useEffect, useState } from "react";
import { invokeCommand } from "../../lib/tauri";
import type { RecoveryClassification, ProjectRecoveryState, ProviderJobView } from "@cinematic/domain";
import { ErrorPanel } from "./ErrorPanel";
import { listProviderJobs } from "../workflows/api";
import { openPanel } from "../../lib/panelNavigation";
import styles from "./JobsPanel.module.css";

interface JobsPanelProps {
  projectRootPath: string;
}

/** Provider-job statuses that mean "still working in the background". */
const ACTIVE_JOB_STATUSES = new Set(["submitted", "polling"]);

/**
 * JobsPanel displays all incomplete jobs and their recovery states, plus the
 * P10.1 durable background provider jobs with live progress. The workflow
 * remains the user-facing source of action; this panel observes and
 * navigates, it never executes.
 */
export const JobsPanel: React.FC<JobsPanelProps> = ({ projectRootPath }) => {
  const [state, setState] = useState<ProjectRecoveryState | null>(null);
  const [providerJobs, setProviderJobs] = useState<ProviderJobView[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    setLoading(true);
    setError(null);
    Promise.all([
      invokeCommand<ProjectRecoveryState>("get_project_recovery_state", { projectRootPath }),
      listProviderJobs(projectRootPath),
    ])
      .then(([recovery, jobs]) => {
        setState(recovery);
        setProviderJobs(jobs);
      })
      .catch((err) => {
        setError(String(err));
      })
      .finally(() => {
        setLoading(false);
      });
  }, [projectRootPath]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const hasActiveProviderJobs = providerJobs.some((job) => ACTIVE_JOB_STATUSES.has(job.status));

  useEffect(() => {
    if (!hasActiveProviderJobs) return;
    const timer = window.setInterval(refresh, 2000);
    return () => window.clearInterval(timer);
  }, [hasActiveProviderJobs, refresh]);

  if (loading && !state) {
    return (
      <div className={styles.container}>
        <p className={styles.loading}>Scanning project recovery state...</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className={styles.container}>
        <div className={styles.error}>Failed to load recovery state: {error}</div>
      </div>
    );
  }

  if (!state) {
    return null;
  }

  const hasIncomplete = state.hasIncompleteJobs || providerJobs.length > 0;

  if (!hasIncomplete) {
    return (
      <div className={styles.container}>
        <p className={styles.allClear}>All jobs completed. Project is ready to use.</p>
      </div>
    );
  }

  return (
    <div className={styles.container}>
      <div className={styles.header}>
        <h2>Background Activity</h2>
        <p className={styles.subtitle}>
          {providerJobs.length > 0
            ? `${providerJobs.length} provider job${providerJobs.length !== 1 ? "s" : ""}`
            : `${state.classifications.length} incomplete job${state.classifications.length !== 1 ? "s" : ""}`}
        </p>
      </div>

      {providerJobs.length > 0 ? (
        <section aria-label="Provider generation jobs" className={styles.jobsList}>
          {providerJobs.map((job) => (
            <ProviderJobCard key={job.id} job={job} />
          ))}
        </section>
      ) : null}

      {state.classifications.length > 0 ? (
        <div className={styles.jobsList}>
          {state.classifications.map((classification) => (
            <JobCard key={classification.jobId} classification={classification} />
          ))}
        </div>
      ) : null}
    </div>
  );
};

/** Human labels for the durable provider job lifecycle. */
function jobStatusLabel(status: ProviderJobView["status"]): string {
  switch (status) {
    case "submitted":
      return "Submitted";
    case "polling":
      return "Working";
    case "completed":
      return "Completed";
    case "failed":
      return "Failed";
    case "cancelled":
      return "Cancelled";
    default:
      return status;
  }
}

interface ProviderJobCardProps {
  job: ProviderJobView;
}

/**
 * ProviderJobCard shows one durable background job: provider, model,
 * operation, status, progress, timing, and attempt/run context, with an
 * "Open workflow" navigation to the owning run.
 */
const ProviderJobCard: React.FC<ProviderJobCardProps> = ({ job }) => {
  const active = ACTIVE_JOB_STATUSES.has(job.status);
  const operation = job.operationId;
  return (
    <div className={`${styles.jobCard} ${active ? styles.jobCardActive : ""}`}>
      <div className={styles.jobCardHeader}>
        <div className={styles.jobCardTitle}>
          <span className={styles.jobType}>{job.providerId}</span>
          <span className={styles.jobId}>{job.modelId}</span>
          <span className={`${styles.disposition} ${styles[`disposition-${job.status}`] ?? ""}`}>
            {jobStatusLabel(job.status)}
          </span>
        </div>
        {active && typeof job.progressPercent === "number" ? (
          <span className={styles.progress} role="status">{job.progressPercent}%</span>
        ) : null}
      </div>
      <div className={styles.jobCardContent}>
        <dl className={styles.jobMeta}>
          <div><dt>Operation</dt><dd>{operation}</dd></div>
          <div><dt>Attempt</dt><dd>{job.attemptNumber}</dd></div>
          <div><dt>Started</dt><dd>{new Date(job.submittedAt).toLocaleString()}</dd></div>
          <div><dt>Last update</dt><dd>{new Date(job.updatedAt).toLocaleString()}</dd></div>
          <div><dt>Job id</dt><dd><code>{job.providerJobId}</code></dd></div>
        </dl>
        <button
          type="button"
          className={styles.openWorkflow}
          onClick={() => openPanel("workflows")}
        >
          Open workflow
        </button>
      </div>
    </div>
  );
};

interface JobCardProps {
  classification: RecoveryClassification;
}

/**
 * JobCard displays a single job's classification and recovery state.
 */
const JobCard: React.FC<JobCardProps> = ({ classification }) => {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className={styles.jobCard}>
      <div
        className={styles.jobCardHeader}
        onClick={() => setExpanded(!expanded)}
        role="button"
        tabIndex={0}
        aria-expanded={expanded}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            setExpanded(!expanded);
          }
        }}
      >
        <div className={styles.jobCardTitle}>
          <span className={styles.jobType}>{classification.jobType}</span>
          <span className={styles.jobId}>{classification.jobId}</span>
          <span className={`${styles.disposition} ${styles[`disposition-${classification.disposition}`]}`}>
            {classification.disposition.replace(/_/g, " ")}
          </span>
        </div>
        <span className={styles.expandIcon}>{expanded ? "▼" : "▶"}</span>
      </div>

      {expanded && (
        <div className={styles.jobCardContent}>
          <ErrorPanel
            jobType={classification.jobType}
            explanation={classification.explanation}
            userAction={classification.userAction}
            preservedFailureInfo={classification.preservedFailureInfo}
          />
        </div>
      )}
    </div>
  );
};
