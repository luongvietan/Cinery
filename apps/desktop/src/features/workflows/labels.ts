import type { WorkflowRunRecord } from "@cinematic/domain";

/** Human-facing labels for run statuses. Raw status strings are internal
 * vocabulary and never render directly outside "Technical details". */
const RUN_STATUS_LABELS: Record<string, string> = {
  created: "Preparing",
  running: "Generating",
  waiting_for_approval: "Needs your approval",
  ready_for_execution: "Ready to generate",
  completed: "Done",
  rejected: "Stopped",
  cancelled: "Cancelled",
  failed: "Failed",
};

const STEP_STATUS_LABELS: Record<string, string> = {
  pending: "Waiting",
  running: "Working",
  waiting: "Waiting",
  completed: "Done",
  skipped: "Skipped",
  failed: "Failed",
};

export function runStatusLabel(status: string): string {
  return RUN_STATUS_LABELS[status] ?? status;
}

export function stepStatusLabel(status: string): string {
  return STEP_STATUS_LABELS[status] ?? status;
}

/** Maps internal workflow step ids to the production step a filmmaker
 * understands. The id remains available in "Technical details". */
const STEP_LABELS: Record<string, string> = {
  validate_input: "Check inputs",
  resolve_context: "Collect references",
  compile_request: "Prepare the request",
  approval: "Your approval",
  execute: "Generate",
  complete: "Finish",
};

export function stepLabel(stepDefinitionId: string): string {
  return STEP_LABELS[stepDefinitionId] ?? stepDefinitionId;
}

/** Maps operation ids to the plain name of the thing being generated. */
const OPERATION_LABELS: Record<string, string> = {
  "character.create_face_lock": "Face reference",
  "character.create_outfit": "Outfit",
  "character.create_character_sheet": "Character sheet",
  "world.create_plate": "World backdrop",
  "scene.create_keyframe": "Shot keyframe",
  "scene.generate_video": "Scene video",
  "shot.image_to_video": "Shot video",
  "asset.run_visual_qa": "Visual QA",
  "asset.repair_failed_qa": "Repair",
};

export function operationLabel(operationId: string): string {
  return OPERATION_LABELS[operationId] ?? operationId;
}

/** History rows show "what was made" + when, not skill/operation ids. */
export function runTitle(run: Pick<WorkflowRunRecord, "operationId" | "createdAt">): string {
  return operationLabel(run.operationId);
}

export function formatRunTime(isoDate: string): string {
  const date = new Date(isoDate);
  if (Number.isNaN(date.getTime())) return isoDate;
  return date.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}
