import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CharacterEditor } from "./CharacterEditor";
import { getCanonEntity } from "./api";
vi.mock("./api");
beforeEach(() => vi.mocked(getCanonEntity).mockResolvedValue({ entity: { id: "character-1", projectId: "project-1", type: "character", name: "Mara Keene", slug: "mara-keene", createdAt: "now", updatedAt: "now" }, sections: [] }));
describe("CharacterEditor", () => { it("exposes narrative and visual-lock sections", async () => { render(<CharacterEditor projectRootPath="/projects/red-door" entityId="character-1" />); expect(await screen.findByText("Role Tag")).toBeInTheDocument(); expect(screen.getByText("Visual Locks")).toBeInTheDocument(); expect(screen.getByText("Sub-beats")).toBeInTheDocument(); }); });
