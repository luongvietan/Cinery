export interface Scene {
  id: string;
  projectId: string;
  ordinal: number;
  title: string;
  summary: string;
  worldId: string | null;
  worldAssetVersionId: string | null;
  keyframeAssetId: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface SceneCharacterAssignment {
  id: string;
  sceneId: string;
  characterEntityId: string;
  lookAssetVersionId: string;
  sheetAssetVersionId: string | null;
  notes: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface ScenePropAssignment {
  id: string;
  sceneId: string;
  propAssetVersionId: string;
  label: string | null;
  notes: string | null;
  createdAt: string;
}

export type SceneReferenceHealth =
  | "current"
  | "upgrade_available"
  | "historical"
  | "broken";

export interface ResolvedSceneReference {
  assetId: string;
  pinnedVersionId: string;
  currentCanonicalVersionId: string | null;
  health: SceneReferenceHealth;
  versionNumber: number;
  status: string;
  filePath: string;
}

export interface ResolvedCharacterReference {
  assignmentId: string;
  characterEntityId: string;
  look: ResolvedSceneReference;
  sheet: ResolvedSceneReference | null;
}

export interface ResolvedPropReference {
  assignmentId: string;
  reference: ResolvedSceneReference;
}

export interface ResolvedSceneReferences {
  sceneId: string;
  world: ResolvedSceneReference | null;
  characters: ResolvedCharacterReference[];
  props: ResolvedPropReference[];
}

export type TbdDecisionKind = "preserve_unknown" | "not_applicable";

export interface SceneTbdBinding {
  id: string;
  sceneId: string;
  canonTbdId: string;
  topicSnapshot: string;
  noteSnapshot: string | null;
  decision: TbdDecisionKind;
  justification: string | null;
  createdAt: string;
  updatedAt: string;
}

export type SceneReadinessBlockerKind =
  | "title_missing"
  | "summary_missing"
  | "world_reference_missing"
  | "world_reference_broken"
  | "character_reference_broken"
  | "prop_reference_broken"
  | "tbd_decision_required";

export type SceneReadinessWarningKind =
  | "upgrade_available"
  | "historical_reference";

export interface SceneReadinessBlocker {
  kind: SceneReadinessBlockerKind;
  message: string;
  context?: string | null;
}

export interface SceneReadinessWarning {
  kind: SceneReadinessWarningKind;
  message: string;
  context?: string | null;
}

export interface SceneReadiness {
  readyForKeyframe: boolean;
  blockers: SceneReadinessBlocker[];
  warnings: SceneReadinessWarning[];
}

export function formatSceneOrdinal(ordinal: number): string {
  return `SCENE-${String(ordinal).padStart(3, "0")}`;
}

export function formatVersionLabel(versionNumber: number): string {
  return `V${String(versionNumber).padStart(2, "0")}`;
}
