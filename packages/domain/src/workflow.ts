import type { CanonEntityType } from "./canon";
import type { CanonTbd } from "./tbd";
import type { Prerequisite } from "./skill";

export const WORKFLOW_RUN_STATUSES = [
  "created",
  "running",
  "waiting_for_approval",
  "ready_for_execution",
  "completed",
  "rejected",
  "cancelled",
  "failed",
] as const;
export type WorkflowRunStatus = (typeof WORKFLOW_RUN_STATUSES)[number];

export const WORKFLOW_STEP_STATUSES = [
  "pending",
  "running",
  "waiting",
  "completed",
  "skipped",
  "failed",
] as const;
export type WorkflowStepStatus = (typeof WORKFLOW_STEP_STATUSES)[number];

export const WORKFLOW_EVENT_TYPES = [
  "run_created",
  "run_started",
  "step_started",
  "step_completed",
  "approval_requested",
  "approval_granted",
  "approval_rejected",
  "execution_started",
  "execution_completed",
  "run_completed",
  "run_cancelled",
  "run_failed",
] as const;
export type WorkflowEventType = (typeof WORKFLOW_EVENT_TYPES)[number];

export interface PrerequisiteCheck {
  id: string;
  prerequisite: Prerequisite;
  status: "pass" | "fail";
  message: string;
  resolvedRef: string | null;
}

export interface PrerequisiteReport {
  passed: boolean;
  checks: PrerequisiteCheck[];
}

export interface CanonSnapshotRef {
  entityId: string;
  entityType: CanonEntityType;
  sectionId: string;
  sectionKey: string;
  revision: number;
  status: "locked";
  value: unknown;
}

export interface AssetSnapshotRef {
  assetId: string;
  assetVersionId: string;
  assetType: string;
  versionNumber: number;
  status: "canonical";
  path: string;
}

export type CanonTbdSnapshot = CanonTbd;

export interface WorkflowContextSnapshot {
  snapshotVersion: 1;
  project: { projectId: string };
  skill: { skillId: string; skillVersion: string; operationId: string };
  input: unknown;
  prerequisiteReport: PrerequisiteReport;
  canon: CanonSnapshotRef[];
  assets: AssetSnapshotRef[];
  protectedTbds: CanonTbdSnapshot[];
  resolvedContext: unknown;
  capturedAt: string;
}

import { z } from "zod";

const prerequisiteCheckSchema = z
  .object({
    id: z.string().min(1),
    prerequisite: z.unknown(),
    status: z.enum(["pass", "fail"]),
    message: z.string(),
    resolvedRef: z.string().nullable(),
  })
  .strict();

export const workflowContextSnapshotSchema = z
  .object({
    snapshotVersion: z.literal(1),
    project: z.object({ projectId: z.string().min(1) }).strict(),
    skill: z
      .object({
        skillId: z.string().min(1),
        skillVersion: z.string().min(1),
        operationId: z.string().min(1),
      })
      .strict(),
    input: z.unknown(),
    prerequisiteReport: z
      .object({ passed: z.boolean(), checks: z.array(prerequisiteCheckSchema) })
      .strict(),
    canon: z.array(z.unknown()),
    assets: z.array(z.unknown()),
    protectedTbds: z.array(z.unknown()),
    resolvedContext: z.unknown(),
    capturedAt: z.string().datetime({ offset: true }),
  })
  .strict();
