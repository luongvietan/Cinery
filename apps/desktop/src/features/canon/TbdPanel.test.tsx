import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TbdPanel } from "./TbdPanel";
import { listCanonEntities, listCanonTbds, resolveCanonTbd } from "./api";
vi.mock("./api");
beforeEach(() => { vi.mocked(listCanonEntities).mockResolvedValue([]); vi.mocked(listCanonTbds).mockResolvedValue([{ id: "tbd-1", projectId: "project-1", canonEntityId: null, sectionKey: null, topic: "What is behind the red door?", note: null, protected: true, status: "open", resolutionText: null, createdAt: "now", updatedAt: "now", resolvedAt: null }]); vi.mocked(resolveCanonTbd).mockResolvedValue({ id: "tbd-1", projectId: "project-1", canonEntityId: null, sectionKey: null, topic: "What is behind the red door?", note: null, protected: true, status: "resolved", resolutionText: "A room.", createdAt: "now", updatedAt: "now", resolvedAt: "now" }); });
describe("TbdPanel", () => { it("requires a non-empty resolution before resolving", async () => { const user = userEvent.setup(); render(<TbdPanel projectRootPath="/projects/red-door" />); expect(await screen.findByText("PROTECTED")).toBeInTheDocument(); const resolve = screen.getByRole("button", { name: "Resolve" }); await user.click(resolve); expect(resolveCanonTbd).not.toHaveBeenCalled(); const input = screen.getByRole("textbox", { name: /Resolution for/ }); await user.type(input, "A room."); await user.click(resolve); expect(resolveCanonTbd).toHaveBeenCalledWith(expect.objectContaining({ resolutionText: "A room." })); }); });
