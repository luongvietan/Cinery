import type { CinemaCompilationRecord, SceneDetail, SceneRecord } from "@cinematic/domain";
import { invokeCommand } from "../../lib/tauri";

export function listScenes(projectRootPath: string): Promise<SceneRecord[]> {
  return invokeCommand("list_scenes", { projectRootPath });
}

export function getScene(projectRootPath: string, sceneId: string): Promise<SceneDetail> {
  return invokeCommand("get_scene", { projectRootPath, sceneId });
}

export function createScene(projectRootPath: string, title: string, worldAssetVersionId: string): Promise<SceneRecord> {
  return invokeCommand("create_scene", { projectRootPath, title, worldAssetVersionId, canonNotes: null });
}

export function stageScene(projectRootPath: string, title: string, worldAssetVersionId: string, characterEntityId: string, lookAssetVersionId: string, sheetAssetVersionId: string): Promise<SceneRecord> {
  return invokeCommand("stage_scene", { projectRootPath, title, worldAssetVersionId, characterEntityId, lookAssetVersionId, sheetAssetVersionId });
}

export function addSceneCharacter(projectRootPath: string, sceneId: string, characterEntityId: string, lookAssetVersionId: string, sheetAssetVersionId: string): Promise<SceneDetail> {
  return invokeCommand("add_scene_character", { projectRootPath, sceneId, characterEntityId, lookAssetVersionId, sheetAssetVersionId });
}

export function createShot(projectRootPath: string, sceneId: string, intent: string): Promise<SceneDetail["shots"][number]> {
  return invokeCommand("create_shot", { projectRootPath, sceneId, ordering: null, durationSeconds: 4, intent, action: null, camera: null });
}

export function compileCinema(projectRootPath: string, sceneId: string, totalDurationSeconds: number): Promise<CinemaCompilationRecord> {
  return invokeCommand("compile_cinema", { projectRootPath, sceneId, totalDurationSeconds, shotCount: null });
}

// --- Complete cinema workspace command surface ---

export function createSceneFull(projectRootPath: string, title: string, worldAssetVersionId: string | null): Promise<SceneRecord> {
  return invokeCommand("create_scene", { projectRootPath, title, worldAssetVersionId, canonNotes: null });
}

export function renameScene(projectRootPath: string, sceneId: string, title: string): Promise<SceneRecord> {
  return invokeCommand("rename_scene", { projectRootPath, sceneId, title });
}

export function setSceneWorld(projectRootPath: string, sceneId: string, worldAssetVersionId: string | null): Promise<void> {
  return invokeCommand("set_scene_world", { projectRootPath, sceneId, worldAssetVersionId });
}

export function updateSceneCharacter(projectRootPath: string, sceneId: string, characterEntityId: string, lookAssetVersionId: string | null, sheetAssetVersionId: string | null): Promise<void> {
  return invokeCommand("update_scene_character", { projectRootPath, sceneId, characterEntityId, lookAssetVersionId, sheetAssetVersionId });
}

export function removeSceneCharacter(projectRootPath: string, sceneId: string, characterEntityId: string): Promise<void> {
  return invokeCommand("remove_scene_character", { projectRootPath, sceneId, characterEntityId });
}

export function addSceneProp(projectRootPath: string, sceneId: string, propAssetVersionId: string): Promise<SceneDetail> {
  return invokeCommand("add_scene_prop", { projectRootPath, sceneId, propAssetVersionId });
}

export function removeSceneProp(projectRootPath: string, sceneId: string, propAssetVersionId: string): Promise<void> {
  return invokeCommand("remove_scene_prop", { projectRootPath, sceneId, propAssetVersionId });
}

export function updateShot(projectRootPath: string, shotId: string, durationSeconds: number | null, intent: string | null, action: string | null, camera: string | null): Promise<SceneDetail["shots"][number]> {
  return invokeCommand("update_shot", { projectRootPath, shotId, durationSeconds, intent, action, camera });
}

export function deleteShot(projectRootPath: string, sceneId: string, shotId: string): Promise<void> {
  return invokeCommand("delete_shot", { projectRootPath, sceneId, shotId });
}

export function reorderShots(projectRootPath: string, sceneId: string, orderedShotIds: string[]): Promise<SceneDetail["shots"]> {
  return invokeCommand("reorder_shots", { projectRootPath, sceneId, orderedShotIds });
}

export function setShotKeyframe(projectRootPath: string, shotId: string, keyframeAssetVersionId: string | null): Promise<void> {
  return invokeCommand("set_shot_keyframe", { projectRootPath, shotId, keyframeAssetVersionId });
}

export interface CinemaReadiness {
  sceneId: string;
  ready: boolean;
  blockers: Array<{
    code: string;
    sceneId: string;
    entityId: string | null;
    shotId: string | null;
    message: string;
    actionTarget: "world" | "cast" | "props" | "shot";
  }>;
}

export function getSceneReadiness(projectRootPath: string, sceneId: string): Promise<CinemaReadiness> {
  return invokeCommand("get_scene_readiness", { projectRootPath, sceneId });
}
