import type { WorkflowRunRecord, ProviderExecutionSummary } from "../workflow";
import type { AssetVersion } from "../asset";

/**
 * Disposition enum for job recovery classification.
 * Used to determine what action to take when a project reopens with incomplete jobs.
 */
export type RecoveryDisposition =
  | "nothing_required"           // Job completed or safe state; no action needed
  | "resume_local"               // Local operation can safely resume (e.g., repair in progress)
  | "await_user_retry"           // User must explicitly retry (provider/cloud failure)
  | "inspect_remote_result"      // Fetch remote state before deciding next step
  | "manual_resolution_required"; // Broken state requiring user intervention

/**
 * Preserved failure information from QA runs.
 * Allows restart to show exactly what failed without re-running QA.
 */
export interface PreservedQaFailure {
  checks: Array<{
    id: string;
    type: string;
    status: "pass" | "fail" | "uncertain" | "not_applicable";
    label: string;
  }>;
}

/**
 * Classification result for a single incomplete job.
 * Explains what happened, why, what state is safe, and what user can do.
 */
export interface RecoveryClassification {
  jobType: "workflow" | "provider" | "qa" | "repair" | "cinema_compile";
  jobId: string;
  disposition: RecoveryDisposition;
  explanation: string; // WHAT happened, WHY it didn't complete
  preservedFailureInfo?: PreservedQaFailure; // For QA failures
  parentVersionId?: string; // For repair: parent asset version ID
  userAction: null | "explicit_retry" | "inspect_and_repair" | "complete_repair"; // WHAT user can do
}

/**
 * Payload returned by the get_project_recovery_state command.
 * Lists every incomplete job with its recovery classification.
 */
export interface ProjectRecoveryState {
  projectId: string;
  classifications: RecoveryClassification[];
  hasIncompleteJobs: boolean;
}

/**
 * Union type for job classification input.
 * Handles all job types: workflow, provider, QA, repair, cinema.
 */
export type JobForRecovery =
  | {
      type: "workflow";
      run: WorkflowRunRecord;
    }
  | {
      type: "provider";
      execution: ProviderExecutionSummary;
      assetVersionCreated: boolean; // Safety check: did provider failure create phantom asset?
    }
  | {
      type: "qa";
      assetVersion: AssetVersion;
      qaFailureDetails?: { checks: PreservedQaFailure["checks"] };
    }
  | {
      type: "repair";
      parentVersion: AssetVersion;
      childVersion: AssetVersion;
    }
  | {
      type: "cinema_compile";
      compilation: {
        id: string;
        cinemaId: string;
        status: "completed" | "failed" | "cancelled";
        resultJson: string;
        createdAt: string;
      };
    };

/**
 * Classifies a single job and determines recovery disposition.
 *
 * Rules:
 * - Approval waiting: nothing_required (safe, not auto-approved)
 * - Provider failed without asset: await_user_retry (explicit only)
 * - Provider failed WITH asset: manual_resolution_required (phantom asset!)
 * - QA failed: nothing_required (failure preserved, can repair)
 * - Repair in progress: resume_local (can continue locally)
 * - Cancelled: nothing_required (terminal, cannot resume)
 * - Cinema compile completed: nothing_required (deterministic, inspectable)
 */
export function classifyJobRecovery(job: JobForRecovery): RecoveryClassification {
  switch (job.type) {
    case "workflow":
      return classifyWorkflowRecovery(job.run);

    case "provider":
      return classifyProviderRecovery(job.execution, job.assetVersionCreated);

    case "qa":
      return classifyQaRecovery(job.assetVersion, job.qaFailureDetails);

    case "repair":
      return classifyRepairRecovery(job.parentVersion, job.childVersion);

    case "cinema_compile":
      return classifyCinemaCompileRecovery(job.compilation);
  }
}

function classifyWorkflowRecovery(run: WorkflowRunRecord): RecoveryClassification {
  // Terminal states
  if (run.status === "completed") {
    return {
      jobType: "workflow",
      jobId: run.id,
      disposition: "nothing_required",
      explanation: "Workflow completed successfully.",
      userAction: null,
    };
  }

  if (run.status === "cancelled") {
    return {
      jobType: "workflow",
      jobId: run.id,
      disposition: "nothing_required",
      explanation: "Workflow was cancelled and remains in that state. It cannot be accidentally resumed.",
      userAction: null,
    };
  }

  if (run.status === "rejected") {
    return {
      jobType: "workflow",
      jobId: run.id,
      disposition: "nothing_required",
      explanation: "Workflow execution was rejected during validation.",
      userAction: null,
    };
  }

  if (run.status === "failed") {
    return {
      jobType: "workflow",
      jobId: run.id,
      disposition: "nothing_required",
      explanation: `Workflow failed: ${run.failureMessage || "Unknown error"}. The project state is safe. You can start a new workflow run.`,
      userAction: null,
    };
  }

  // Non-terminal states
  if (run.status === "waiting_for_approval") {
    return {
      jobType: "workflow",
      jobId: run.id,
      disposition: "nothing_required",
      explanation: `Workflow is awaiting approval at step ${run.currentStepIndex}. Your canonical data was not changed. Provide approval to continue or start a new run.`,
      userAction: null,
    };
  }

  if (run.status === "created") {
    return {
      jobType: "workflow",
      jobId: run.id,
      disposition: "nothing_required",
      explanation: "Workflow was created but never started.",
      userAction: null,
    };
  }

  if (run.status === "running" || run.status === "ready_for_execution") {
    return {
      jobType: "workflow",
      jobId: run.id,
      disposition: "nothing_required",
      explanation: `Workflow execution was interrupted. The canonical data was not mutated. You can inspect the incomplete run or start a new one.`,
      userAction: null,
    };
  }

  return {
    jobType: "workflow",
    jobId: run.id,
    disposition: "nothing_required",
    explanation: "Workflow is in an unknown state.",
    userAction: null,
  };
}

function classifyProviderRecovery(
  execution: ProviderExecutionSummary,
  assetVersionCreated: boolean,
): RecoveryClassification {
  // Success: nothing to do
  if (execution.status === "succeeded") {
    return {
      jobType: "provider",
      jobId: execution.id,
      disposition: "nothing_required",
      explanation: "Provider call succeeded.",
      userAction: null,
    };
  }

  // Cancelled: terminal
  if (execution.status === "cancelled" || execution.status === "cancellation_requested") {
    return {
      jobType: "provider",
      jobId: execution.id,
      disposition: "nothing_required",
      explanation: "Provider call was cancelled and cannot be resumed.",
      userAction: null,
    };
  }

  // Failed: critical check for phantom asset
  if (execution.status === "failed") {
    if (assetVersionCreated) {
      // CRITICAL: phantom asset created on failure
      return {
        jobType: "provider",
        jobId: execution.id,
        disposition: "manual_resolution_required",
        explanation: `Provider failed, but an AssetVersion was created (phantom asset). This should not happen. ${execution.normalizedErrorJson || "No error details available."}`,
        userAction: null,
      };
    }

    // Safe: no asset created, retry explicitly
    return {
      jobType: "provider",
      jobId: execution.id,
      disposition: "await_user_retry",
      explanation: `Provider failed: ${execution.normalizedErrorJson || "Unknown error"}. no output asset was created. Retry only via explicit user action.`,
      userAction: "explicit_retry",
    };
  }

  // In-progress states: queued, submitted, running
  if (execution.status === "queued" || execution.status === "submitted" || execution.status === "running") {
    return {
      jobType: "provider",
      jobId: execution.id,
      disposition: "inspect_remote_result",
      explanation: `Provider call is in progress (${execution.status}). Check remote provider status and fetch result.`,
      userAction: null,
    };
  }

  // Unknown
  return {
    jobType: "provider",
    jobId: execution.id,
    disposition: "inspect_remote_result",
    explanation: "Provider call status is unknown. Fetch remote provider status.",
    userAction: null,
  };
}

function classifyQaRecovery(
  version: AssetVersion,
  failureDetails?: { checks: PreservedQaFailure["checks"] },
): RecoveryClassification {
  // QA failed: preserve failure info, keep asset in qa_failed state
  if (version.status === "qa_failed") {
    return {
      jobType: "qa",
      jobId: version.id,
      disposition: "nothing_required",
      explanation: `QA failed for asset version ${version.versionNumber}. The failure details are preserved. You can repair the asset or create a new version.`,
      preservedFailureInfo: failureDetails,
      userAction: "inspect_and_repair",
    };
  }

  // QA in progress: repairing state
  if (version.status === "repairing") {
    return {
      jobType: "qa",
      jobId: version.id,
      disposition: "resume_local",
      explanation: "Asset repair is in progress.",
      userAction: "complete_repair",
    };
  }

  // Other states: nothing to do
  return {
    jobType: "qa",
    jobId: version.id,
    disposition: "nothing_required",
    explanation: `Asset version ${version.id} is in ${version.status} state.`,
    userAction: null,
  };
}

function classifyRepairRecovery(
  parentVersion: AssetVersion,
  childVersion: AssetVersion,
): RecoveryClassification {
  // Repair is resumable locally
  if (childVersion.status === "repairing" && childVersion.parentVersionId === parentVersion.id) {
    return {
      jobType: "repair",
      jobId: childVersion.id,
      disposition: "resume_local",
      explanation: `Repair of asset version ${parentVersion.versionNumber} was interrupted. The parent-child relationship is preserved. You can continue or abandon the repair.`,
      parentVersionId: parentVersion.id,
      userAction: "complete_repair",
    };
  }

  return {
    jobType: "repair",
    jobId: childVersion.id,
    disposition: "nothing_required",
    explanation: `Repair child relationship is intact: parent ${parentVersion.id} → child ${childVersion.id}.`,
    parentVersionId: parentVersion.id,
    userAction: null,
  };
}

function classifyCinemaCompileRecovery(compilation: {
  id: string;
  cinemaId: string;
  status: "completed" | "failed" | "cancelled";
  resultJson: string;
  createdAt: string;
}): RecoveryClassification {
  // Completed: deterministic and inspectable
  if (compilation.status === "completed") {
    return {
      jobType: "cinema_compile",
      jobId: compilation.id,
      disposition: "nothing_required",
      explanation: `Cinema compilation completed and cached. The deterministic result remains inspectable and available for export.`,
      userAction: null,
    };
  }

  // Failed/cancelled
  return {
    jobType: "cinema_compile",
    jobId: compilation.id,
    disposition: "nothing_required",
    explanation: `Cinema compilation ${compilation.status}. You can retry the compilation.`,
    userAction: null,
  };
}
