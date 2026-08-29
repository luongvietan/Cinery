import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { GooeyNav, GooeyNavItem } from "./GooeyNav";

describe("GooeyNav", () => {
  it("renders nav items as toggle buttons inside the liquid group", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(
      <GooeyNav ariaLabel="Workspace panels">
        <GooeyNavItem
          label="Overview"
          pressed={true}
          className="nav-button nav-button--active"
          onClick={() => onSelect("overview")}
        />
        <GooeyNavItem
          label="Assets"
          pressed={false}
          className="nav-button"
          onClick={() => onSelect("assets")}
        />
      </GooeyNav>,
    );

    const overview = screen.getByRole("button", { name: "Overview" });
    const assets = screen.getByRole("button", { name: "Assets" });

    expect(overview).toHaveAttribute("aria-pressed", "true");
    expect(overview).toHaveClass("nav-button--active");
    expect(assets).toHaveAttribute("aria-pressed", "false");

    await user.click(assets);
    expect(onSelect).toHaveBeenCalledWith("assets");
  });
});
