import { invokeCommand } from "../../lib/tauri";
import type {
  ResolvedSceneReference,
  ResolvedSceneReferences,
  Scene,
  SceneCharacterAssignment,
  ScenePropAssignment,
} from "./types";

export function createScene(
  projectRootPath: string,
  title: string,
  summary: string,
): Promise<Scene> {
  return invokeCommand<Scene>("create_world_scene", {
    projectRootPath,
    title,
    summary,
  });
}

export function listScenes(projectRootPath: string): Promise<Scene[]> {
  return invokeCommand<Scene[]>("list_world_scenes", { projectRootPath });
}

export function getScene(
  projectRootPath: string,
  sceneId: string,
): Promise<Scene> {
  return invokeCommand<Scene>("get_world_scene", { projectRootPath, sceneId });
}

export function updateSceneDetails(
  projectRootPath: string,
  sceneId: string,
  title: string,
  summary: string,
): Promise<Scene> {
  return invokeCommand<Scene>("update_scene_details", {
    projectRootPath,
    sceneId,
    title,
    summary,
  });
}

export function assignSceneWorld(
  projectRootPath: string,
  sceneId: string,
  worldId: string,
): Promise<Scene> {
  return invokeCommand<Scene>("assign_scene_world", {
    projectRootPath,
    sceneId,
    worldId,
  });
}

export function clearSceneWorld(
  projectRootPath: string,
  sceneId: string,
): Promise<Scene> {
  return invokeCommand<Scene>("clear_scene_world", {
    projectRootPath,
    sceneId,
  });
}

export function addSceneCharacter(
  projectRootPath: string,
  sceneId: string,
  characterEntityId: string,
  lookAssetVersionId: string,
  sheetAssetVersionId?: string | null,
  notes?: string | null,
): Promise<SceneCharacterAssignment> {
  return invokeCommand<SceneCharacterAssignment>("add_world_scene_character", {
    projectRootPath,
    sceneId,
    characterEntityId,
    lookAssetVersionId,
    sheetAssetVersionId: sheetAssetVersionId ?? null,
    notes: notes ?? null,
  });
}

export function removeSceneCharacter(
  projectRootPath: string,
  sceneId: string,
  characterEntityId: string,
): Promise<void> {
  return invokeCommand<void>("remove_world_scene_character", {
    projectRootPath,
    sceneId,
    characterEntityId,
  });
}

export function listSceneCharacters(
  projectRootPath: string,
  sceneId: string,
): Promise<SceneCharacterAssignment[]> {
  return invokeCommand<SceneCharacterAssignment[]>("list_scene_characters", {
    projectRootPath,
    sceneId,
  });
}

export function addSceneProp(
  projectRootPath: string,
  sceneId: string,
  propAssetVersionId: string,
  label?: string | null,
  notes?: string | null,
): Promise<ScenePropAssignment> {
  return invokeCommand<ScenePropAssignment>("add_world_scene_prop", {
    projectRootPath,
    sceneId,
    propAssetVersionId,
    label: label ?? null,
    notes: notes ?? null,
  });
}

export function removeSceneProp(
  projectRootPath: string,
  sceneId: string,
  propAssetVersionId: string,
): Promise<void> {
  return invokeCommand<void>("remove_world_scene_prop", {
    projectRootPath,
    sceneId,
    propAssetVersionId,
  });
}

export function listSceneProps(
  projectRootPath: string,
  sceneId: string,
): Promise<ScenePropAssignment[]> {
  return invokeCommand<ScenePropAssignment[]>("list_scene_props", {
    projectRootPath,
    sceneId,
  });
}

export function resolveSceneReferences(
  projectRootPath: string,
  sceneId: string,
): Promise<ResolvedSceneReferences> {
  return invokeCommand<ResolvedSceneReferences>("resolve_scene_references", {
    projectRootPath,
    sceneId,
  });
}

export function upgradeSceneWorldReference(
  projectRootPath: string,
  sceneId: string,
): Promise<ResolvedSceneReference> {
  return invokeCommand<ResolvedSceneReference>(
    "upgrade_scene_world_reference",
    { projectRootPath, sceneId },
  );
}

export function upgradeSceneCharacterLookReference(
  projectRootPath: string,
  sceneId: string,
  assignmentId: string,
): Promise<ResolvedSceneReference> {
  return invokeCommand<ResolvedSceneReference>(
    "upgrade_scene_character_look_reference",
    { projectRootPath, sceneId, assignmentId },
  );
}

export function upgradeSceneCharacterSheetReference(
  projectRootPath: string,
  sceneId: string,
  assignmentId: string,
): Promise<ResolvedSceneReference> {
  return invokeCommand<ResolvedSceneReference>(
    "upgrade_scene_character_sheet_reference",
    { projectRootPath, sceneId, assignmentId },
  );
}

export function upgradeScenePropReference(
  projectRootPath: string,
  sceneId: string,
  assignmentId: string,
): Promise<ResolvedSceneReference> {
  return invokeCommand<ResolvedSceneReference>(
    "upgrade_scene_prop_reference",
    { projectRootPath, sceneId, assignmentId },
  );
}
