import { z } from "zod";
import type { CanonEntityType } from "./canon";

function uniqueFieldValues<T>(
  items: T[],
  selector: (item: T) => string,
  fieldName: string,
): boolean {
  const seen = new Set<string>();
  for (const item of items) {
    const value = selector(item);
    if (seen.has(value)) {
      return false;
    }
    seen.add(value);
  }
  return true;
}

const trimmedString = z.string().transform((value) => value.trim());

const optionalText = trimmedString;

const requiredText = trimmedString.pipe(z.string().min(1));

const requiredId = trimmedString.pipe(z.string().min(1));

export const premiseSchema = z.object({
  text: optionalText,
});

export const thesisSchema = z.object({
  text: optionalText,
});

export const timelineEntrySchema = z.object({
  id: requiredId,
  label: optionalText,
  description: optionalText,
});

export const timelineSchema = z
  .object({
    entries: z.array(timelineEntrySchema),
  })
  .superRefine((value, ctx) => {
    if (!uniqueFieldValues(value.entries, (entry) => entry.id, "id")) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: "Timeline entry IDs must be unique",
      });
    }
  });

export const aestheticSchema = z.object({
  visualRegister: optionalText,
  palette: z.array(trimmedString),
  materials: z.array(trimmedString),
  lighting: optionalText,
  atmosphere: optionalText,
  exteriorPresence: optionalText,
  anomalyRule: optionalText,
  notes: z.array(trimmedString),
});

export const relationshipsSchema = z.object({
  text: optionalText,
});

export const structuralEngineSchema = z.object({
  id: requiredId,
  title: optionalText,
  description: optionalText,
});

export const structuralEnginesSchema = z
  .object({
    engines: z.array(structuralEngineSchema),
  })
  .superRefine((value, ctx) => {
    if (!uniqueFieldValues(value.engines, (engine) => engine.id, "id")) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: "Structural engine IDs must be unique",
      });
    }
  });

export const activeSkillRulesSchema = z.object({
  text: optionalText,
});

export const roleTagSchema = z.object({
  text: optionalText,
});

export const visualSummarySchema = z.object({
  text: optionalText,
});

export const functionSchema = z.object({
  text: optionalText,
});

export const backstorySchema = z.object({
  text: optionalText,
});

export const psychologySchema = z.object({
  text: optionalText,
});

export const promptReadyDescriptorSchema = z.object({
  text: optionalText,
});

export const visualLockSchema = z.object({
  id: requiredId,
  key: requiredId,
  description: requiredText,
  severity: z.enum(["required", "important"]),
  validatorHint: z.string().nullable(),
});

export const visualLocksSchema = z
  .object({
    locks: z.array(visualLockSchema),
  })
  .superRefine((value, ctx) => {
    if (!uniqueFieldValues(value.locks, (lock) => lock.key, "key")) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: "Visual lock keys must be unique",
      });
    }
  });

export const characterSubBeatSchema = z.object({
  id: requiredId,
  title: optionalText,
  text: optionalText,
});

export const subBeatsSchema = z
  .object({
    beats: z.array(characterSubBeatSchema),
  })
  .superRefine((value, ctx) => {
    if (!uniqueFieldValues(value.beats, (beat) => beat.id, "id")) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: "Sub-beat IDs must be unique",
      });
    }
  });

export const locationDescriptionSchema = z.object({
  text: optionalText,
});

export const visualTagsSchema = z.object({
  tags: z.array(trimmedString),
});

export const geographySchema = z.object({
  text: optionalText,
});

export const locationRulesSchema = z.object({
  rules: z.array(trimmedString),
});

export const factionTextSchema = z.object({
  text: optionalText,
});

export const worldRuleSchema = z.object({
  text: optionalText,
});

export const worldRuleNotesSchema = z.object({
  text: optionalText,
});

export const productionRuleSchema = z.object({
  id: requiredId,
  title: optionalText,
  body: optionalText,
});

export const productionRulesSchema = z
  .object({
    rules: z.array(productionRuleSchema),
  })
  .superRefine((value, ctx) => {
    if (!uniqueFieldValues(value.rules, (rule) => rule.id, "id")) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: "Production rule IDs must be unique",
      });
    }
  });

export const CANON_SECTION_SCHEMAS = {
  story: {
    premise: premiseSchema,
    thesis: thesisSchema,
    timeline: timelineSchema,
    aesthetic: aestheticSchema,
    relationships: relationshipsSchema,
    structural_engines: structuralEnginesSchema,
    active_skill_rules: activeSkillRulesSchema,
  },
  character: {
    role_tag: roleTagSchema,
    visual_summary: visualSummarySchema,
    function: functionSchema,
    backstory: backstorySchema,
    psychology: psychologySchema,
    speech: promptReadyDescriptorSchema,
    movement: promptReadyDescriptorSchema,
    stillness: promptReadyDescriptorSchema,
    visual_locks: visualLocksSchema,
    sub_beats: subBeatsSchema,
  },
  location: {
    description: locationDescriptionSchema,
    visual_tags: visualTagsSchema,
    geography: geographySchema,
    rules: locationRulesSchema,
  },
  faction: {
    description: factionTextSchema,
    visual_signature: factionTextSchema,
    public_face: factionTextSchema,
    actual_behavior: factionTextSchema,
  },
  world_rule: {
    rule: worldRuleSchema,
    notes: worldRuleNotesSchema,
  },
  production_rules: {
    rules: productionRulesSchema,
  },
} as const satisfies Record<
  CanonEntityType,
  Record<string, z.ZodType<unknown>>
>;

export function parseCanonSectionValue(
  entityType: CanonEntityType,
  sectionKey: string,
  value: unknown,
): unknown {
  const entitySchemas = CANON_SECTION_SCHEMAS[entityType];
  const schema = (entitySchemas as Record<string, z.ZodType<unknown>>)[
    sectionKey
  ];
  if (!schema) {
    throw new Error(`Unknown canon section for entity type: ${sectionKey}`);
  }
  return schema.parse(value);
}
