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

  it("supports video (P10.0) but still rejects audio", () => {
    expect(validateSprintOneAssetType("video")).toBe("video");
    expect(() => validateSprintOneAssetType("audio")).toThrow(
      "This asset type is not supported yet",
    );
  });
});
