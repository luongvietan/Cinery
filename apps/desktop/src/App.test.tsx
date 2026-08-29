import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import App from "./App";

describe("App", () => {
  it("renders the product shell", () => {
    render(<App />);
    expect(
      screen.getByRole("heading", {
        name: "Make films with AI, without losing your characters.",
      }),
    ).toBeInTheDocument();
  });
});
