import { render, screen, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ActionButton } from "./ActionButton";

describe("ActionButton", () => {
  it("renders an enabled button without a reason", () => {
    const onClick = vi.fn();
    render(<ActionButton onClick={onClick}>Save</ActionButton>);

    const button = screen.getByRole("button", { name: "Save" });
    expect(button).not.toBeDisabled();
    expect(screen.queryByRole("note")).not.toBeInTheDocument();

    fireEvent.click(button);
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("renders an enabled button even when a reason is provided", () => {
    render(
      <ActionButton disabled={false} disabledReason="unused">
        Go
      </ActionButton>,
    );

    expect(screen.getByRole("button", { name: "Go" })).not.toBeDisabled();
    expect(screen.queryByRole("note")).not.toBeInTheDocument();
  });

  it("shows the reason next to a disabled button", () => {
    render(
      <ActionButton disabled disabledReason="Requires a canonical Face.">
        Create Outfit
      </ActionButton>,
    );

    expect(screen.getByRole("button", { name: "Create Outfit" })).toBeDisabled();
    expect(screen.getByRole("note")).toHaveTextContent(
      "Requires a canonical Face.",
    );
  });

  it("keeps disabled buttons inert", () => {
    const onClick = vi.fn();
    render(
      <ActionButton onClick={onClick} disabled disabledReason="busy">
        Compile
      </ActionButton>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Compile" }));
    expect(onClick).not.toHaveBeenCalled();
  });
});
