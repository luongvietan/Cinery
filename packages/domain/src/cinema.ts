import { z } from "zod";

/**
 * One shot inside a compiled cinema prompt. Provider-neutral: carries no
 * provider or model identifiers -- only deterministic creative constraints.
 */
export const shotInstructionSchema = z.object({
  order: z.number().int().min(0),
  durationSeconds: z.number().positive().max(30),
  intent: z.string().trim().min(1).max(240),
  camera: z.string().trim().min(1).max(160).nullish(),
  action: z.string().trim().min(1).max(240).nullish(),
  continuity: z.string().trim().min(1).max(600).nullish(),
});

export type ShotInstruction = z.infer<typeof shotInstructionSchema>;

export const behavioralLocksSchema = z.object({
  speech: z.string().trim().min(1).nullish(),
  movement: z.string().trim().min(1).nullish(),
  stillness: z.string().trim().min(1).nullish(),
});

export type BehavioralLocks = z.infer<typeof behavioralLocksSchema>;

export const worldContinuitySchema = z.object({
  plateId: z.string().trim().min(1).nullish(),
  plateAssetVersionId: z.string().trim().min(1).nullish(),
  description: z.string().trim().min(1).nullish(),
});

export type WorldContinuity = z.infer<typeof worldContinuitySchema>;

/**
 * The persisted result of one cinema compilation run. `providerPrompt` is
 * the full provider-neutral text; shots must sum to `totalDurationSeconds`.
 */
export const cinemaCompilationSchema = z
  .object({
    id: z.string().trim().min(1),
    projectId: z.string().trim().min(1),
    sceneId: z.string().trim().min(1),
    totalDurationSeconds: z.number().min(1).max(120),
    shots: z.array(shotInstructionSchema).min(1),
    behavioralLocks: behavioralLocksSchema,
    worldContinuity: worldContinuitySchema,
    continuityNotes: z.string().trim().min(1).nullish(),
    audioInstructions: z.string().trim().min(1).nullish(),
    lastFrame: z.string().trim().min(1).nullish(),
    providerPrompt: z.string().trim().min(1),
    createdAt: z.string().trim().min(1),
  })
  .superRefine((value, ctx) => {
    const sum = value.shots.reduce(
      (acc, shot) => acc + shot.durationSeconds,
      0,
    );
    if (Math.abs(sum - value.totalDurationSeconds) > 1e-6) {
      ctx.addIssue({
        code: "custom",
        message:
          "shot durations must sum to totalDurationSeconds " +
          `(${sum} != ${value.totalDurationSeconds})`,
        path: ["shots"],
      });
    }
  });

export type CinemaCompilation = z.infer<typeof cinemaCompilationSchema>;

export interface SceneRecord {
  id: string;
  projectId: string;
  title: string;
  worldAssetVersionId: string | null;
  canonNotes: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface SceneCharacterRecord {
  characterEntityId: string;
  lookAssetVersionId: string;
  sheetAssetVersionId: string | null;
  displayOrder: number;
}

export interface ScenePropRecord {
  propAssetVersionId: string;
  displayOrder: number;
}

export interface ShotRecord {
  id: string;
  sceneId: string;
  ordering: number;
  durationSeconds: number;
  keyframeAssetVersionId: string | null;
  /** Exact, immutable generated-video AssetVersion pin (P10.0). Promoting
   * a newer video version never rewrites this reference. */
  generatedVideoAssetVersionId: string | null;
  intent: string;
  action: string | null;
  camera: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface SceneDetail {
  scene: SceneRecord;
  characters: SceneCharacterRecord[];
  props: ScenePropRecord[];
  shots: ShotRecord[];
}

/** Result of pinning one exact promoted I2V candidate version onto a Shot. */
export interface ShotVideoPromotionResult {
  shotId: string;
  artifactId: string;
  assetVersionId: string;
  previousAssetVersionId: string | null;
}

/** Display-only projection of a Shot's exact pinned keyframe for the I2V
 * panel: the frozen source the generation run will use. */
export interface ShotImageToVideoSource {
  assetId: string;
  assetVersionId: string;
  versionNumber: number;
  filePath: string;
  thumbnailPath: string | null;
  mimeType: string;
}

export interface CinemaCompilationRecord {
  id: string;
  projectId: string;
  sceneId: string;
  inputJson: string;
  compilationJson: string;
  exportPath: string;
  exportSha256: string;
  createdAt: string;
}

/**
 * Validates a total compilation runtime: the master plan bounds P8 scenes
 * between 1 and 120 seconds.
 */
export function validateTotalDuration(durationSeconds: number): number {
  if (
    !Number.isFinite(durationSeconds) ||
    durationSeconds < 1 ||
    durationSeconds > 120
  ) {
    throw new RangeError(
      "total duration must be between 1 and 120 seconds, got " +
        String(durationSeconds),
    );
  }
  return durationSeconds;
}

/**
 * Splits `totalSeconds` across `shotCount` shots (auto-sized at ~4s per
 * shot when omitted, minimum one shot). Deterministic: remainder centiseconds
 * are distributed to earlier shots, and the parts always sum exactly to the
 * validated total.
 */
export function computeTimeBudget(
  totalSeconds: number,
  shotCount?: number,
): number[] {
  validateTotalDuration(totalSeconds);
  const count =
    shotCount === undefined
      ? Math.max(1, Math.ceil(totalSeconds / 4))
      : shotCount;
  if (!Number.isInteger(count) || count < 1 || count > 240) {
    throw new RangeError(`invalid shot count: ${String(shotCount)}`);
  }
  const totalCs = Math.round(totalSeconds * 100);
  const base = Math.floor(totalCs / count);
  const remainder = totalCs % count;
  return Array.from({ length: count }, (_, index) =>
    (base + (index < remainder ? 1 : 0)) / 100,
  );
}
