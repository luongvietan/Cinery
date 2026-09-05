import { invokeCommand } from "../../lib/tauri";
import type {
  ExtensionDirection,
  ExtensionRequest,
  SequenceFlow,
} from "@cinematic/domain";
import type { SequenceBrief } from "@cinematic/domain";

/** Read model of `mark_sequence_references_ready`: blockers or the advance. */
export interface SequenceReferencesReadyReport {
  flow: SequenceFlow | null;
  blockers: Array<{ code: string; message: string }>;
}

/**
 * Typed facade for the sequence-first flow commands (Joey contract). Every
 * function maps 1:1 onto an explicit Tauri command; errors are never hidden
 * and no call is ever retried here.
 */
export function getSequenceFlow(
  projectRootPath: string,
  sceneId: string,
): Promise<SequenceFlow> {
  return invokeCommand<SequenceFlow>("get_sequence_flow", {
    projectRootPath,
    sceneId,
  });
}

export function updateSequenceBrief(
  projectRootPath: string,
  sceneId: string,
  brief: SequenceBrief,
): Promise<SequenceFlow> {
  return invokeCommand<SequenceFlow>("update_sequence_brief", {
    projectRootPath,
    sceneId,
    brief,
  });
}

export function markSequenceReferencesReady(
  projectRootPath: string,
  sceneId: string,
): Promise<SequenceReferencesReadyReport> {
  return invokeCommand<SequenceReferencesReadyReport>(
    "mark_sequence_references_ready",
    { projectRootPath, sceneId },
  );
}

export function approveSequencePreflight(
  projectRootPath: string,
  sceneId: string,
  approvedCompilationId: string | null,
): Promise<SequenceFlow> {
  return invokeCommand<SequenceFlow>("approve_sequence_preflight", {
    projectRootPath,
    sceneId,
    approvedCompilationId,
  });
}

export function beginSequenceReview(
  projectRootPath: string,
  sceneId: string,
): Promise<SequenceFlow> {
  return invokeCommand<SequenceFlow>("begin_sequence_review", {
    projectRootPath,
    sceneId,
  });
}

export function prepareSequenceExtension(
  projectRootPath: string,
  sceneId: string,
  direction: ExtensionDirection,
): Promise<ExtensionRequest> {
  return invokeCommand<ExtensionRequest>("prepare_sequence_extension", {
    projectRootPath,
    sceneId,
    direction,
  });
}
