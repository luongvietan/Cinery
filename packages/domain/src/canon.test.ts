import { describe, expect, it } from "vitest";
import { CANON_ENTITY_TYPES } from "./canon";

describe("canon entity types", () => {
  it("includes all required entity types", () => {
    expect(CANON_ENTITY_TYPES).toEqual([
      "story",
      "character",
      "location",
      "faction",
      "world_rule",
      "production_rules",
    ]);
  });
});
