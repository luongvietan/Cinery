import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CreateWorldButton } from "./CreateWorldButton";
import { createWorld, listWorlds } from "./api";
import { listCanonEntities } from "../canon/api";

vi.mock("./api");
vi.mock("../canon/api");

describe("CreateWorldButton", () => {
  beforeEach(() => {
    vi.mocked(createWorld).mockReset();
    vi.mocked(listWorlds).mockReset();
    vi.mocked(listCanonEntities).mockReset();
  });

  it("disables locations already used by a World", async () => {
    vi.mocked(listCanonEntities).mockResolvedValue([
      { id: "loc-1", projectId: "p", type: "location", name: "The Station", slug: "the-station", createdAt: "now", updatedAt: "now" },
      { id: "loc-2", projectId: "p", type: "location", name: "Rooftop", slug: "rooftop", createdAt: "now", updatedAt: "now" },
    ] as any);
    vi.mocked(listWorlds).mockResolvedValue([
      { id: "world-1", projectId: "p", canonLocationEntityId: "loc-1", worldPlateAssetId: "asset-1", createdAt: "now", updatedAt: "now" },
    ] as any);

    const user = userEvent.setup();
    render(
      <CreateWorldButton projectRootPath="/projects/red-door" onCreated={vi.fn()} />,
    );

    await user.click(screen.getByRole("button", { name: "New World" }));
    expect(await screen.findByRole("heading", { name: "Create World" })).toBeInTheDocument();

    const optionStation = screen.getByRole("option", { name: /The Station/ }) as HTMLOptionElement;
    const optionRooftop = screen.getByRole("option", { name: /Rooftop/ }) as HTMLOptionElement;

    expect(optionStation.disabled).toBe(true);
    expect(optionRooftop.disabled).toBe(false);
    expect(optionStation.textContent).toContain("already has World");
  });

  it("creates world with selected location", async () => {
    vi.mocked(listCanonEntities).mockResolvedValue([
      { id: "loc-2", projectId: "p", type: "location", name: "Rooftop", slug: "rooftop", createdAt: "now", updatedAt: "now" },
    ] as any);
    vi.mocked(listWorlds).mockResolvedValue([] as any);
    vi.mocked(createWorld).mockResolvedValue({
      id: "world-2",
      projectId: "p",
      canonLocationEntityId: "loc-2",
      worldPlateAssetId: "asset-2",
      createdAt: "now",
      updatedAt: "now",
    } as any);

    const onCreated = vi.fn();
    const user = userEvent.setup();
    render(<CreateWorldButton projectRootPath="/projects/red-door" onCreated={onCreated} />);

    await user.click(screen.getByRole("button", { name: "New World" }));
    await screen.findByRole("heading", { name: "Create World" });

    // select should auto-select the only available location, but ensure
    await user.selectOptions(screen.getByLabelText("Canon Location"), "loc-2");
    await user.click(screen.getByRole("button", { name: "Create World" }));
    expect(createWorld).toHaveBeenCalledWith("/projects/red-door", "loc-2");
    expect(onCreated).toHaveBeenCalledWith("world-2");
  });
});
