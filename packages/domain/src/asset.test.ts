import { describe, expect, it } from "vitest";
import {
  formatVersionNumber,
  validateAssetLabel,
  validateSprintOneAssetType,
} from "./asset";

describe("asset domain", () => {
  it("formats version numbers for display", () => {
    expect(formatVersionNumber(1)).toBe("v001");
    expect(formatVersionNumber(12)).toBe("v012");
    expect(formatVersionNumber(105)).toBe("v105");
  });

  it("trims a valid label", () => {
    expect(validateAssetLabel("  MARA-FACE  ")).toBe("MARA-FACE");
  });

  it("rejects video and audio in Sprint 1", () => {
    expect(() => validateSprintOneAssetType("video")).toThrow(
      "This asset type is not supported in Sprint 1",
    );
    expect(() => validateSprintOneAssetType("audio")).toThrow(
      "This asset type is not supported in Sprint 1",
    );
  });
});
