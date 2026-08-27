import { describe, expect, it } from "vitest";
import {
  parseCanonSectionValue,
  premiseSchema,
  visualLockSchema,
  visualLocksSchema,
} from "./canon-schema";

describe("canon schemas", () => {
  it("accepts a valid premise", () => {
    expect(
      premiseSchema.parse({
        text: "A lone operator receives her own future voice.",
      }),
    ).toEqual({
      text: "A lone operator receives her own future voice.",
    });
  });

  it("parses a section through the entity schema registry", () => {
    expect(
      parseCanonSectionValue("character", "role_tag", { text: "Lead" }),
    ).toEqual({ text: "Lead" });
  });

  it("rejects an invalid visual lock severity", () => {
    expect(() =>
      visualLockSchema.parse({
        id: "scar",
        key: "right_eyebrow_scar",
        description: "Scar on character-right eyebrow",
        severity: "optional",
        validatorHint: null,
      }),
    ).toThrow();
  });

  it("rejects duplicate visual lock keys", () => {
    expect(() =>
      visualLocksSchema.parse({
        locks: [
          {
            id: "one",
            key: "scar",
            description: "A",
            severity: "required",
            validatorHint: null,
          },
          {
            id: "two",
            key: "scar",
            description: "B",
            severity: "important",
            validatorHint: null,
          },
        ],
      }),
    ).toThrow();
  });
});
