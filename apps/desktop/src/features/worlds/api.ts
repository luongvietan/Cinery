import type { WorkflowRunDetail } from "@cinematic/domain";
import { invokeCommand } from "../../lib/tauri";
import type { TbdDecision, World, WorldDetail } from "./types";

export function createWorld(
  projectRootPath: string,
  canonLocationEntityId: string,
): Promise<World> {
  return invokeCommand<World>("create_world", {
    projectRootPath,
    canonLocationEntityId,
  });
}

export function listWorlds(projectRootPath: string): Promise<World[]> {
  return invokeCommand<World[]>("list_worlds", { projectRootPath });
}

export function getWorld(
  projectRootPath: string,
  worldId: string,
): Promise<World> {
  return invokeCommand<World>("get_world", { projectRootPath, worldId });
}

export function listWorldsDetailed(
  projectRootPath: string,
): Promise<WorldDetail[]> {
  return invokeCommand<WorldDetail[]>("list_worlds_detailed", {
    projectRootPath,
  });
}

export function getWorldDetailed(
  projectRootPath: string,
  worldId: string,
): Promise<WorldDetail> {
  return invokeCommand<WorldDetail>("get_world_detailed", {
    projectRootPath,
    worldId,
  });
}

export function createWorldPlateWorkflowRun(
  projectRootPath: string,
  worldId: string,
  tbdDecisions: TbdDecision[] = [],
  providerId?: string,
  modelId?: string,
): Promise<WorkflowRunDetail> {
  return invokeCommand<WorkflowRunDetail>("create_workflow_run", {
    projectRootPath,
    skillId: "world-builder",
    skillVersion: "1.0.0",
    operationId: "world.create_plate",
    input: { worldId, tbdDecisions, providerId, modelId },
  });
}
