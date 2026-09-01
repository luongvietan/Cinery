import { invokeCommand } from "../../lib/tauri";
import type { ShotImageToVideoSource, ShotVideoPromotionResult } from "@cinematic/domain";
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

// ---------------------------------------------------------------------------
// Shots + cinema compilation on the unified Scene (backend `cinema` module)
// ---------------------------------------------------------------------------

export interface Shot {
  id: string;
  sceneId: string;
  ordering: number;
  durationSeconds: number;
  keyframeAssetVersionId: string | null;
  /** Exact, immutable generated-video AssetVersion pin (P10.0). */
  generatedVideoAssetVersionId: string | null;
  intent: string;
  action: string | null;
  camera: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface CinemaCompilation {
  id: string;
  projectId: string;
  sceneId: string;
  inputJson: string;
  compilationJson: string;
  exportPath: string;
  exportSha256: string;
  createdAt: string;
}

export interface CompileReadinessBlocker {
  code: string;
  sceneId: string;
  entityId: string | null;
  shotId: string | null;
  message: string;
  actionTarget: string;
}

export interface CompileReadiness {
  sceneId: string;
  ready: boolean;
  blockers: CompileReadinessBlocker[];
}

export function listShots(projectRootPath: string, sceneId: string): Promise<Shot[]> {
  return invokeCommand<Shot[]>("list_shots", { projectRootPath, sceneId });
}

export function createShot(
  projectRootPath: string,
  sceneId: string,
  durationSeconds: number,
  intent: string,
  action?: string | null,
  camera?: string | null,
): Promise<Shot> {
  return invokeCommand<Shot>("create_shot", {
    projectRootPath,
    sceneId,
    ordering: null,
    durationSeconds,
    intent,
    action: action ?? null,
    camera: camera ?? null,
  });
}

export function updateShot(
  projectRootPath: string,
  shotId: string,
  fields: { durationSeconds?: number | null; intent?: string | null; action?: string | null; camera?: string | null },
): Promise<Shot> {
  return invokeCommand<Shot>("update_shot", {
    projectRootPath,
    shotId,
    durationSeconds: fields.durationSeconds ?? null,
    intent: fields.intent ?? null,
    action: fields.action ?? null,
    camera: fields.camera ?? null,
  });
}

export function deleteShot(projectRootPath: string, sceneId: string, shotId: string): Promise<void> {
  return invokeCommand<void>("delete_shot", { projectRootPath, sceneId, shotId });
}

export function reorderShots(
  projectRootPath: string,
  sceneId: string,
  orderedShotIds: string[],
): Promise<Shot[]> {
  return invokeCommand<Shot[]>("reorder_shots", { projectRootPath, sceneId, orderedShotIds });
}

export function setShotKeyframe(
  projectRootPath: string,
  shotId: string,
  keyframeAssetVersionId: string | null,
): Promise<void> {
  return invokeCommand<void>("set_shot_keyframe", {
    projectRootPath,
    shotId,
    keyframeAssetVersionId,
  });
}

/** Pins (or clears) the shot's exact generated-video AssetVersion. The
 * reference never drifts when newer video versions are promoted. */
export function setShotVideo(
  projectRootPath: string,
  shotId: string,
  videoAssetVersionId: string | null,
): Promise<void> {
  return invokeCommand<void>("set_shot_video", {
    projectRootPath,
    shotId,
    videoAssetVersionId,
  });
}

/** Display-only projection of the Shot's exact pinned keyframe — the frozen
 * source an image-to-video run will use. */
export function getShotImageToVideoSource(
  projectRootPath: string,
  shotId: string,
): Promise<ShotImageToVideoSource> {
  return invokeCommand<ShotImageToVideoSource>("get_shot_image_to_video_source", {
    projectRootPath,
    shotId,
  });
}

/** Promotes one exact captured `shot.image_to_video` candidate onto the
 * Shot's video pin under explicit human review. Conflict-safe: a stale
 * expected pin rejects with PROMOTION_CONFLICT. */
export function promoteShotVideoCandidate(
  projectRootPath: string,
  shotId: string,
  artifactId: string,
  expectedCurrentVideoAssetVersionId: string | null,
): Promise<ShotVideoPromotionResult> {
  return invokeCommand<ShotVideoPromotionResult>("promote_shot_video_candidate", {
    projectRootPath,
    shotId,
    artifactId,
    expectedCurrentVideoAssetVersionId,
  });
}

// ---------------------------------------------------------------------------
// Scene TBD bindings (persisted decisions; see SceneTbdPanel)
// ---------------------------------------------------------------------------

export interface SceneTbdBindingRecord {
  id: string;
  sceneId: string;
  canonTbdId: string;
  topicSnapshot: string;
  noteSnapshot: string | null;
  decision: "preserve_unknown" | "not_applicable";
  justification: string | null;
  createdAt: string;
  updatedAt: string;
}

export function setSceneTbdBinding(
  projectRootPath: string,
  sceneId: string,
  tbdId: string,
  decision: "preserve_unknown" | "not_applicable",
  justification: string | null,
): Promise<SceneTbdBindingRecord> {
  return invokeCommand<SceneTbdBindingRecord>("set_scene_tbd_binding", {
    projectRootPath,
    sceneId,
    tbdId,
    decision,
    justification,
  });
}

export function removeSceneTbdBinding(
  projectRootPath: string,
  sceneId: string,
  tbdId: string,
): Promise<void> {
  return invokeCommand<void>("remove_scene_tbd_binding", {
    projectRootPath,
    sceneId,
    tbdId,
  });
}

export function listSceneTbdBindings(
  projectRootPath: string,
  sceneId: string,
): Promise<SceneTbdBindingRecord[]> {
  return invokeCommand<SceneTbdBindingRecord[]>("list_scene_tbd_bindings", {
    projectRootPath,
    sceneId,
  });
}

export function getCompileReadiness(
  projectRootPath: string,
  sceneId: string,
): Promise<CompileReadiness> {
  return invokeCommand<CompileReadiness>("get_scene_readiness", { projectRootPath, sceneId });
}

export function compileCinema(
  projectRootPath: string,
  sceneId: string,
  totalDurationSeconds: number,
): Promise<CinemaCompilation> {
  return invokeCommand<CinemaCompilation>("compile_cinema", {
    projectRootPath,
    sceneId,
    totalDurationSeconds,
    shotCount: null,
  });
}

export function listCinemaCompilations(
  projectRootPath: string,
  sceneId: string,
): Promise<CinemaCompilation[]> {
  return invokeCommand<CinemaCompilation[]>("list_cinema_compilations", {
    projectRootPath,
    sceneId,
  });
}

export function ensureSceneKeyframeAsset(
  projectRootPath: string,
  sceneId: string,
): Promise<{ id: string; label: string }> {
  return invokeCommand<{ id: string; label: string }>("ensure_scene_keyframe_asset", {
    projectRootPath,
    sceneId,
  });
}
