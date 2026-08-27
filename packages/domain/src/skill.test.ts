import { describe, expect, it } from "vitest";
import {
  skillDefinitionSchema,
  workflowStepDefinitionSchema,
} from "./skill";

describe("skill contracts", () => {
  it("accepts the versioned face-lock operation and rejects unknown fields", () => {
    const definition = {
      id: "character-builder",
      name: "Character Builder",
      version: "1.0.0",
      description: "Build character production assets.",
      operations: [
        {
          id: "character.create_face_lock",
          name: "Create Face Lock",
          description: "Create a canonical face-lock request.",
          intentExamples: ["lock Mara's face"],
          inputSchemaId: "create_face_lock",
          prerequisites: [
            {
              type: "canon_entity_exists",
              entityType: "character",
              inputRef: "characterEntityId",
            },
          ],
          tbdGuards: [],
          workflow: [
            { id: "validate-input", type: "validate_input" },
            {
              id: "resolve-context",
              type: "resolve_context",
              resolverId: "character_face_lock_context",
            },
          ],
          expectedOutput: {
            assetType: "face_lock",
            mediaType: "image",
            desiredStatus: "candidate",
            ownerEntityInputRef: "characterEntityId",
          },
        },
      ],
    };

    expect(skillDefinitionSchema.parse(definition).version).toBe("1.0.0");
    expect(() =>
      skillDefinitionSchema.parse({ ...definition, unexpected: true }),
    ).toThrow();
  });

  it("rejects an unsupported workflow step discriminator", () => {
    expect(() =>
      workflowStepDefinitionSchema.parse({
        id: "run-script",
        type: "script",
      }),
    ).toThrow();
  });
});
