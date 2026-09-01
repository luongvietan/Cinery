export type QaRunStatus = "queued" | "running" | "succeeded" | "failed" | "cancelled";
export type QaOverallStatus = "pass" | "fail" | "needs_review";
export type QaMediaKind = "image" | "video";
export type QaCheckStatus = "pass" | "fail" | "uncertain" | "not_applicable";
export type QaCheckSource =
  | "visual_lock"
  | "canonical_reference"
  | "operation_expectation"
  | "artifact_detection";
export type QaReviewStatus =
  | "unreviewed"
  | "confirmed"
  | "overridden_pass"
  | "overridden_fail";

export type QaCheckType =
  | "identity_similarity"
  | "permanent_visual_lock"
  | "hair_consistency"
  | "skin_register"
  | "outfit_piece"
  | "accessory_placement"
  | "required_element"
  | "forbidden_element"
  | "background_requirement"
  | "composition_requirement"
  | "watermark"
  | "unexpected_artifact"
  | "video_integrity"
  | "start_frame_continuity"
  | "identity_temporal_consistency"
  | "reference_temporal_consistency"
  | "motion_adherence"
  | "camera_motion_adherence"
  | "temporal_coherence"
  | "unexpected_cut"
  | "flicker"
  | "deformation_or_warping";

export interface QaCheckDefinition {
  id: string;
  checkType: QaCheckType;
  source: QaCheckSource;
  key: string;
  label: string;
  requirement: string;
  validatorHint: string | null;
  blocking: boolean;
  referenceAssetVersionIds: string[];
}

export interface QaCheckPlan {
  schemaVersion: 1;
  assetId: string;
  assetVersionId: string;
  ownerEntityId: string | null;
  assetType: string;
  referenceAssetVersionIds: string[];
  checks: QaCheckDefinition[];
  createdAt: string;
}

export interface QaCheckResult {
  checkId: string;
  status: QaCheckStatus;
  confidence: number | null;
  observed: string;
  reason: string;
  repairHint: string | null;
}

export interface VisualQaResult {
  schemaVersion: 1;
  checks: QaCheckResult[];
  modelSummary: string | null;
}

export interface QaCheckRecord extends QaCheckResult {
  id: string;
  qaRunId: string;
  checkType: QaCheckType;
  source: QaCheckSource;
  requirement: unknown;
  reviewStatus: QaReviewStatus;
  reviewNote: string | null;
  reviewedAt: string | null;
  createdAt: string;
}

export interface QaRunRecord {
  id: string;
  projectId: string;
  assetId: string;
  assetVersionId: string;
  mediaKind: QaMediaKind;
  workflowRunId: string | null;
  status: QaRunStatus;
  overallStatus: QaOverallStatus | null;
  adapterId: string | null;
  adapterVersion: string | null;
  modelId: string | null;
  executionLocation: string;
  checkPlan: QaCheckPlan;
  contextSnapshot: unknown;
  rawResponseMetadata: unknown | null;
  errorCode: string | null;
  errorMessage: string | null;
  createdAt: string;
  startedAt: string | null;
  completedAt: string | null;
}

export interface QaRunDetail {
  run: QaRunRecord;
  checks: QaCheckRecord[];
}
