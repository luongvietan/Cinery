import type { WorkflowRunDetail } from "@cinematic/domain";
import { invokeCommand } from "../../lib/tauri";
import type { QaReviewStatus, QaRunDetail, QaRunRecord } from "./types";
import { qaRunDetailSchema, qaRunRecordSchema } from "./schemas";

export async function listQaRuns(
  projectRootPath: string,
  assetVersionId: string,
): Promise<QaRunRecord[]> {
  const value = await invokeCommand<unknown>("list_qa_runs", { projectRootPath, assetVersionId });
  return qaRunRecordSchema.array().parse(value) as QaRunRecord[];
}

export async function getQaRun(
  projectRootPath: string,
  qaRunId: string,
): Promise<QaRunDetail> {
  const value = await invokeCommand<unknown>("get_qa_run", { projectRootPath, qaRunId });
  return qaRunDetailSchema.parse(value) as QaRunDetail;
}

export async function reviewQaCheck(input: {
  projectRootPath: string;
  qaRunId: string;
  checkId: string;
  reviewStatus: QaReviewStatus;
  note: string | null;
}): Promise<QaRunDetail> {
  const value = await invokeCommand<unknown>("review_qa_check", input);
  return qaRunDetailSchema.parse(value) as QaRunDetail;
}

export function createVisualQaWorkflow(
  projectRootPath: string,
  assetVersionId: string,
  providerId?: string,
  modelId?: string,
): Promise<WorkflowRunDetail> {
  return invokeCommand("create_workflow_run", {
    projectRootPath,
    skillId: "visual-qa",
    skillVersion: "1.0.0",
    operationId: "asset.run_visual_qa",
    input: {
      projectRootPath,
      assetVersionId,
      adapterId: "openai",
      providerId: providerId || undefined,
      modelId: modelId || undefined,
      expectations: [],
    },
  });
}

/** Creates Video QA through the shared workflow command surface. */
export function createVideoQaWorkflow(
  projectRootPath: string,
  assetVersionId: string,
  providerId?: string,
  modelId?: string,
): Promise<WorkflowRunDetail> {
  return invokeCommand("create_workflow_run", {
    projectRootPath,
    skillId: "video-qa",
    skillVersion: "1.0.0",
    operationId: "asset.run_video_qa",
    input: {
      assetVersionId,
      adapterId: "openai",
      providerId: providerId || undefined,
      modelId: modelId || undefined,
    },
  });
}

export function createVisualRepairWorkflow(
  projectRootPath: string,
  assetVersionId: string,
  qaRunId: string,
  providerId: string,
  modelId: string,
): Promise<WorkflowRunDetail> {
  return invokeCommand("create_workflow_run", {
    projectRootPath,
    skillId: "visual-qa",
    skillVersion: "1.0.0",
    operationId: "asset.repair_failed_qa",
    input: {
      projectRootPath,
      qaRunId,
      providerId,
      modelId,
      qaAdapterId: providerId === "mock" ? "mock" : "openai",
    },
  });
}

export function advanceQaWorkflow(
  projectRootPath: string,
  workflowRunId: string,
): Promise<WorkflowRunDetail> {
  return invokeCommand("advance_workflow_run", { projectRootPath, workflowRunId });
}

export function approveQaWorkflow(
  projectRootPath: string,
  workflowRunId: string,
  stepDefinitionId: string,
  note = "Visual QA media disclosure reviewed",
): Promise<WorkflowRunDetail> {
  return invokeCommand("approve_workflow_step", {
    projectRootPath,
    workflowRunId,
    stepDefinitionId,
    note,
  });
}

export function rejectQaWorkflow(
  projectRootPath: string,
  workflowRunId: string,
  stepDefinitionId = "approve-qa",
): Promise<WorkflowRunDetail> {
  return invokeCommand("reject_workflow_step", {
    projectRootPath,
    workflowRunId,
    stepDefinitionId,
    note: "Visual QA cancelled before execution",
  });
}
