import { describe, expect, it } from "vitest";
import {
  validateProjectName,
  validateProjectRootPath,
} from "./project";

describe("project input validation", () => {
  it("rejects blank project names", () => {
    expect(() => validateProjectName("   ")).toThrow(
      "Project name must contain 1 to 120 characters",
    );
  });

  it("rejects names longer than 120 characters", () => {
    expect(() => validateProjectName("x".repeat(121))).toThrow(
      "Project name must contain 1 to 120 characters",
    );
  });

  it("trims a valid project name", () => {
    expect(validateProjectName("  Red Door  ")).toBe("Red Door");
  });

  it("rejects an empty project path", () => {
    expect(() => validateProjectRootPath(" ")).toThrow(
      "Project path is empty",
    );
  });
});
