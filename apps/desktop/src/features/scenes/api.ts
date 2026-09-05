import { invokeCommand } from "../../lib/tauri";
import type {
  CinemaCompilation as DomainCinemaCompilation,
  SequencePreflight,
  ShotImageToVideoSource,
  ShotVideoPromotionResult,
} from "@cinematic/domain";
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

/** The compiled, provider-neutral prompt stored inside a compilation record. */
interface CompiledPrompt {
  totalDurationSeconds: number;
  providerPrompt: string;
  shots: DomainCinemaCompilation["shots"];
  behavioralLocks?: DomainCinemaCompilation["behavioralLocks"];
  worldContinuity?: DomainCinemaCompilation["worldContinuity"];
  continuity?: string | null;
}

/**
 * Composes the read-only generation disclosure from the existing read-only
 * commands: compile readiness (blockers), resolved references (exact
 * versions), and the latest compilation (full prompt + runtime). No credit
 * estimator exists yet, so `estimatedCredits` honestly reports 0 — the UI
 * labels it "not reported" until a connected service provides one.
 */
export async function buildSequencePreflight(
  projectRootPath: string,
  sceneId: string,
): Promise<SequencePreflight> {
  const [readiness, references, compilations] = await Promise.all([
    getCompileReadiness(projectRootPath, sceneId),
    resolveSceneReferences(projectRootPath, sceneId),
    listCinemaCompilations(projectRootPath, sceneId),
  ]);

  const record = compilations[0] ?? null;
  let compiled: CompiledPrompt | null = null;
  if (record) {
    try {
      compiled = JSON.parse(record.compilationJson) as CompiledPrompt;
    } catch {
      compiled = null;
    }
  }

  const blockers = readiness.blockers.map((blocker) => ({
    code: blocker.code,
    message: blocker.message,
  }));
  if (!record || !compiled) {
    blockers.push({
      code: "compilation_missing",
      message: "Compile the scene prompt before approving generation",
    });
  }

  const totalSeconds = compiled?.totalDurationSeconds ?? 0;
  return {
    sceneId,
    compilation: {
      id: record?.id ?? "",
      projectId: record?.projectId ?? "",
      sceneId,
      totalDurationSeconds: totalSeconds,
      shots: compiled?.shots ?? [],
      behavioralLocks: compiled?.behavioralLocks ?? {},
      worldContinuity: compiled?.worldContinuity ?? {},
      continuityNotes: compiled?.continuity ?? undefined,
      providerPrompt: compiled?.providerPrompt ?? "",
      createdAt: record?.createdAt ?? "",
    },
    providerPrompt: compiled?.providerPrompt ?? "",
    references: [
      ...(references.world && references.world.pinnedVersionId
        ? [{ assetId: references.world.assetId, versionId: references.world.pinnedVersionId, role: "world plate" }]
        : []),
      ...references.characters
        .filter((character) => character.pinnedVersionId)
        .map((character) => ({
          assetId: character.assetId,
          versionId: character.pinnedVersionId as string,
          role: "character look",
        })),
      ...references.props
        .filter((prop) => prop.pinnedVersionId)
        .map((prop) => ({
          assetId: prop.assetId,
          versionId: prop.pinnedVersionId as string,
          role: "prop plate",
        })),
    ],
    estimatedCredits: 0,
    runtimeRecommendation:
      totalSeconds > 15
        ? `Joey recommends ~15-second prompt units: consider splitting this ${totalSeconds}s sequence before generating`
        : `Within Joey's recommended ~15-second prompt unit (${totalSeconds}s)`,
    canGenerate: blockers.length === 0,
    blockers,
  };
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
 * expected pin rejects with PROMOTION_CONFLICT. An exceptional candidate
 * (QA failed/needs-review, stale frozen keyframe) requires a non-empty
 * overrideReason, audited as a QA override. */
export function promoteShotVideoCandidate(
  projectRootPath: string,
  shotId: string,
  artifactId: string,
  expectedCurrentVideoAssetVersionId: string | null,
  overrideReason?: string | null,
): Promise<ShotVideoPromotionResult> {
  return invokeCommand<ShotVideoPromotionResult>("promote_shot_video_candidate", {
    projectRootPath,
    shotId,
    artifactId,
    expectedCurrentVideoAssetVersionId,
    overrideReason: overrideReason ?? null,
  });
}

// ---------------------------------------------------------------------------
// Shot video review (P10.4): candidates, review state, canonical resolver.
// ---------------------------------------------------------------------------

/** Review state of one shot video candidate (orthogonal to canonicality). */
export type CandidateReviewState = "active" | "rejected";

/** One reviewable video candidate resolved server-side. */
export interface ShotVideoCandidate {
  assetVersionId: string;
  versionNumber: number;
  shotId: string;
  sceneId: string;
  createdAt: string;
  filePath: string;
  mimeType: string;
  byteSize: number;
  reviewState: CandidateReviewState;
  isCanonical: boolean;
  qaOverallStatus: string | null;
  qaRunCount: number;
  providerId: string | null;
  modelId: string | null;
  workflowRunId: string | null;
  sourceAssetVersionId: string | null;
  sourceKeyframeIsCurrent: boolean;
}

/** Lists every successful video candidate of a Shot, newest first. */
export function listShotVideoCandidates(
  projectRootPath: string,
  shotId: string,
): Promise<ShotVideoCandidate[]> {
  return invokeCommand<ShotVideoCandidate[]>("list_shot_video_candidates", {
    projectRootPath,
    shotId,
  });
}

/** Resolves the canonical video version for a Shot — the exact promoted
 * pin, or null. Never falls back to the latest generation. */
export function resolveCanonicalShotVideo(
  projectRootPath: string,
  shotId: string,
): Promise<string | null> {
  return invokeCommand<string | null>("resolve_canonical_shot_video", {
    projectRootPath,
    shotId,
  });
}

/** Rejects one video candidate (review state). The canonical candidate
 * cannot be rejected; rejection never deletes artifacts or QA history. */
export function rejectShotVideoCandidate(
  projectRootPath: string,
  shotId: string,
  assetVersionId: string,
  reason: string | null,
): Promise<CandidateReviewState> {
  return invokeCommand<CandidateReviewState>("reject_shot_video_candidate", {
    projectRootPath,
    shotId,
    assetVersionId,
    reason,
  });
}

/** Restores a rejected video candidate to Active. Never promotes. */
export function restoreShotVideoCandidate(
  projectRootPath: string,
  shotId: string,
  assetVersionId: string,
): Promise<CandidateReviewState> {
  return invokeCommand<CandidateReviewState>("restore_shot_video_candidate", {
    projectRootPath,
    shotId,
    assetVersionId,
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
