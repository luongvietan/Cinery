import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { RecoveryClassification, ProjectRecoveryState } from "@cinematic/domain";
import { ErrorPanel } from "./ErrorPanel";
import styles from "./JobsPanel.module.css";

interface JobsPanelProps {
  projectRootPath: string;
}

/**
 * JobsPanel displays all incomplete jobs and their recovery states.
 * Unified background activity surface showing running/pending/failed jobs.
 * No screen-specific hidden job semantics.
 */
export const JobsPanel: React.FC<JobsPanelProps> = ({ projectRootPath }) => {
  const [state, setState] = useState<ProjectRecoveryState | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setLoading(true);
    setError(null);

    invoke<ProjectRecoveryState>("get_project_recovery_state", {
      projectRootPath,
    })
      .then((result) => {
        setState(result);
      })
      .catch((err) => {
        setError(String(err));
      })
      .finally(() => {
        setLoading(false);
      });
  }, [projectRootPath]);

  if (loading) {
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

  if (!state.has_incomplete_jobs) {
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
          {state.classifications.length} incomplete job{state.classifications.length !== 1 ? "s" : ""}
        </p>
      </div>

      <div className={styles.jobsList}>
        {state.classifications.map((classification) => (
          <JobCard key={classification.job_id} classification={classification} />
        ))}
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
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            setExpanded(!expanded);
          }
        }}
      >
        <div className={styles.jobCardTitle}>
          <span className={styles.jobType}>{classification.job_type}</span>
          <span className={styles.jobId}>{classification.job_id}</span>
          <span className={`${styles.disposition} ${styles[`disposition-${classification.disposition}`]}`}>
            {classification.disposition.replace(/_/g, " ")}
          </span>
        </div>
        <span className={styles.expandIcon}>{expanded ? "▼" : "▶"}</span>
      </div>

      {expanded && (
        <div className={styles.jobCardContent}>
          <ErrorPanel
            jobType={classification.job_type}
            explanation={classification.explanation}
            userAction={classification.user_action}
            preservedFailureInfo={classification.preserved_failure_info}
          />
        </div>
      )}
    </div>
  );
};
