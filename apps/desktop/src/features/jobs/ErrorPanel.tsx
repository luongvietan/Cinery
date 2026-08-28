import React from "react";
import type { PreservedQaFailure } from "@cinematic/domain";
import styles from "./ErrorPanel.module.css";

interface ErrorPanelProps {
  jobType: string;
  explanation: string;
  userAction?: string | null;
  preservedFailureInfo?: PreservedQaFailure;
}

/**
 * ErrorPanel explains failures clearly showing:
 * - WHAT happened
 * - WHY action didn't complete
 * - WHAT state remains safe
 * - WHAT user can do next
 */
export const ErrorPanel: React.FC<ErrorPanelProps> = ({
  jobType,
  explanation,
  userAction,
  preservedFailureInfo,
}) => {
  return (
    <div className={styles.container}>
      <div className={styles.explanation}>
        <p>{explanation}</p>
      </div>

      {preservedFailureInfo && (
        <div className={styles.qaFailures}>
          <h4>QA Check Results</h4>
          <div className={styles.checksList}>
            {preservedFailureInfo.checks.map((check) => (
              <div key={check.id} className={`${styles.check} ${styles[`check-${check.status}`]}`}>
                <span className={styles.checkStatus}>{check.status.toUpperCase()}</span>
                <span className={styles.checkLabel}>{check.label}</span>
                <span className={styles.checkType}>({check.check_type})</span>
              </div>
            ))}
          </div>
        </div>
      )}

      {userAction && (
        <div className={styles.action}>
          <p className={styles.actionLabel}>Next Step:</p>
          <p className={styles.actionText}>{formatUserAction(userAction, jobType)}</p>
        </div>
      )}
    </div>
  );
};

function formatUserAction(action: string, jobType: string): string {
  switch (action) {
    case "explicit_retry":
      return "Retry this generation explicitly from the workflow or asset view.";
    case "inspect_and_repair":
      return "Inspect the failed QA checks and start a repair workflow if needed.";
    case "complete_repair":
      return "Continue or complete the in-progress repair.";
    default:
      return "Check the project state and decide on next steps.";
  }
}
