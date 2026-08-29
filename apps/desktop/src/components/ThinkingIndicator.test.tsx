import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ThinkingIndicator } from "./ThinkingIndicator";

describe("ThinkingIndicator", () => {
  it("renders a canvas with an aria-hidden decorative mark", () => {
    const { container } = render(<ThinkingIndicator state="working" />);
    const canvas = container.querySelector("canvas");
    expect(canvas).not.toBeNull();
    expect(canvas).toHaveAttribute("aria-hidden", "true");
    expect(screen.queryByRole("img")).toBeNull();
  });
});
