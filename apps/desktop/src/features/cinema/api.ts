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

export function addSceneCharacter(projectRootPath: string, sceneId: string, characterEntityId: string, lookAssetVersionId: string, sheetAssetVersionId: string): Promise<SceneDetail> {
  return invokeCommand("add_scene_character", { projectRootPath, sceneId, characterEntityId, lookAssetVersionId, sheetAssetVersionId });
}

export function createShot(projectRootPath: string, sceneId: string, intent: string): Promise<SceneDetail["shots"][number]> {
  return invokeCommand("create_shot", { projectRootPath, sceneId, ordering: null, durationSeconds: 4, intent, action: null, camera: null });
}

export function compileCinema(projectRootPath: string, sceneId: string, totalDurationSeconds: number): Promise<CinemaCompilationRecord> {
  return invokeCommand("compile_cinema", { projectRootPath, sceneId, totalDurationSeconds, shotCount: null });
}
