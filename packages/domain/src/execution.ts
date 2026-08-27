import type { AssetType } from "./asset";
import {
  expectedOutputDefinitionSchema,
  type ExpectedOutputDefinition,
} from "./skill";
import { z } from "zod";

export interface ExecutionReference {
  type: "asset_version" | "canon_snapshot";
  reference: string;
  description: string;
}

export const executionReferenceSchema = z
  .object({
    type: z.enum(["asset_version", "canon_snapshot"]),
    reference: z.string().min(1),
    description: z.string().min(1),
  })
  .strict();

export type ExecutionConstraint =
  | { type: "flat_reference_background"; value: "18_percent_neutral_gray" }
  | { type: "shadowless_lighting"; value: true }
  | { type: "no_cast_shadow"; value: true }
  | { type: "no_contact_shadow"; value: true }
  | { type: "no_cinematic_dof"; value: true }
  | { type: "preserve_visual_lock"; key: string; description: string };

export const executionConstraintSchema = z.discriminatedUnion("type", [
  z
    .object({
      type: z.literal("flat_reference_background"),
      value: z.literal("18_percent_neutral_gray"),
    })
    .strict(),
  z.object({ type: z.literal("shadowless_lighting"), value: z.literal(true) }).strict(),
  z.object({ type: z.literal("no_cast_shadow"), value: z.literal(true) }).strict(),
  z.object({ type: z.literal("no_contact_shadow"), value: z.literal(true) }).strict(),
  z.object({ type: z.literal("no_cinematic_dof"), value: z.literal(true) }).strict(),
  z
    .object({
      type: z.literal("preserve_visual_lock"),
      key: z.string().min(1),
      description: z.string().min(1),
    })
    .strict(),
]);

export interface ExecutionRequest {
  requestVersion: 1;
  task:
    | "character_face_lock"
    | "character_outfit"
    | "character_sheet"
    | "world_plate"
    | "shot_keyframe";
  mediaType: "image";
  prompt: string;
  references: ExecutionReference[];
  constraints: ExecutionConstraint[];
  expectedOutput: ExpectedOutputDefinition;
  provenance: {
    workflowRunId: string;
    skillId: string;
    skillVersion: string;
    operationId: string;
  };
}

export const executionRequestSchema = z
  .object({
    requestVersion: z.literal(1),
    task: z.enum([
      "character_face_lock",
      "character_outfit",
      "character_sheet",
      "world_plate",
      "shot_keyframe",
    ]),
    mediaType: z.literal("image"),
    prompt: z.string(),
    references: z.array(executionReferenceSchema),
    constraints: z.array(executionConstraintSchema),
    expectedOutput: expectedOutputDefinitionSchema,
    provenance: z
      .object({
        workflowRunId: z.string().min(1),
        skillId: z.string().min(1),
        skillVersion: z.string().min(1),
        operationId: z.string().min(1),
      })
      .strict(),
  })
  .strict();

export interface ExecutionResult {
  kind: "dry_run";
  artifactPath: string;
  request: ExecutionRequest;
}

export type ExecutionAssetType = AssetType;
