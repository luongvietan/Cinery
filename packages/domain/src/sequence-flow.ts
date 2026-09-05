import { z } from "zod";
import {
  behavioralLocksSchema,
  cinemaCompilationSchema,
  worldContinuitySchema,
} from "./cinema.js";

/**
 * Ordered, explicit stages of the sequence-first workflow. A sequence only
 * advances through these stages by deliberate creator action.
 */
export const sequenceStageSchema = z.enum([
  "draft", "brief_locked", "references_ready", "prompt_approved",
  "generating", "in_review", "canonical_selected", "ready_for_edit",
]);

export type SequenceStage = z.infer<typeof sequenceStageSchema>;

/** The two deliberate continuation directions for extending a canonical take. */
export const extensionDirectionSchema = z.enum(["prequel", "sequel"]);

export type ExtensionDirection = z.infer<typeof extensionDirectionSchema>;

/**
 * The human-authored director brief. Intentionally creator-owned: the AI
 * co-director may suggest, but never writes, this artifact.
 */
export const sequenceBriefSchema = z.object({
  intent: z.string().trim().min(1).max(1000),
  energy: z.enum(["composed", "elevated", "kinetic", "violent"]),
  targetDurationSeconds: z.number().positive().max(120).optional(),
  creditCap: z.number().int().nonnegative(),
});

export type SequenceBrief = z.infer<typeof sequenceBriefSchema>;

/**
 * The persisted sequence-flow record for one scene: the locked brief and the
 * workflow's explicit approval state, keyed by `sceneId`.
 */
export const sequenceFlowSchema = z.object({
  sceneId: z.string().trim().min(1),
  brief: sequenceBriefSchema,
  stage: sequenceStageSchema,
  approvedCompilationId: z.string().trim().min(1).nullish(),
  canonicalShotId: z.string().trim().min(1).nullish(),
  extensionDirection: extensionDirectionSchema.nullish(),
  createdAt: z.string().trim().min(1),
  updatedAt: z.string().trim().min(1),
});

export type SequenceFlow = z.infer<typeof sequenceFlowSchema>;

/** One resolved, version-exact reference disclosed before generation. */
export const sequenceReferenceSchema = z.object({
  assetId: z.string().trim().min(1),
  versionId: z.string().trim().min(1),
  role: z.string().trim().min(1),
});

export type SequenceReference = z.infer<typeof sequenceReferenceSchema>;

/** A production rule or missing continuity anchor that blocks generation. */
export const sequenceBlockerSchema = z.object({
  code: z.string().trim().min(1),
  message: z.string().trim().min(1),
});

export type SequenceBlocker = z.infer<typeof sequenceBlockerSchema>;

/**
 * The read-only generation disclosure: full compiled prompt, resolved
 * references, estimated credit impact, and runtime guidance. Generation is
 * only permitted when `canGenerate` is true and no blockers remain.
 */
export const sequencePreflightSchema = z
  .object({
    sceneId: z.string().trim().min(1),
    compilation: cinemaCompilationSchema,
    providerPrompt: z.string().trim().min(1),
    references: z.array(sequenceReferenceSchema),
    estimatedCredits: z.number().int().nonnegative(),
    runtimeRecommendation: z.string().trim().min(1),
    canGenerate: z.boolean(),
    blockers: z.array(sequenceBlockerSchema),
  })
  .superRefine((value, ctx) => {
    const hasBlockers = value.blockers.length > 0;
    if (value.canGenerate === hasBlockers) {
      ctx.addIssue({
        code: "custom",
        message: "canGenerate must be false exactly when blockers remain",
        path: ["canGenerate"],
      });
    }
  });

export type SequencePreflight = z.infer<typeof sequencePreflightSchema>;

/**
 * The explicit, inspectable input for extending the exact canonical video of
 * a shot in a chosen direction. Preparation only: no provider work is
 * enqueued by creating this request.
 */
export const extensionRequestSchema = z.object({
  sceneId: z.string().trim().min(1),
  shotId: z.string().trim().min(1),
  direction: extensionDirectionSchema,
  canonicalVideoAssetVersionId: z.string().trim().min(1),
  carriedLocks: behavioralLocksSchema,
  worldContinuity: worldContinuitySchema,
  continuationPrompt: z.string().trim().min(1),
});

export type ExtensionRequest = z.infer<typeof extensionRequestSchema>;
