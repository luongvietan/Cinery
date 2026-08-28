import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CanonWorkspace } from "./CanonWorkspace";
import { ensureCanonSingletons, getCanonEntity } from "./api";

vi.mock("./api");

const story = { id: "story-1", projectId: "project-1", type: "story" as const, name: "Story", slug: "story", createdAt: "now", updatedAt: "now" };
beforeEach(() => { vi.mocked(ensureCanonSingletons).mockResolvedValue({ story, productionRules: { ...story, id: "rules-1", type: "production_rules", name: "Production Rules", slug: "production-rules" } }); vi.mocked(getCanonEntity).mockResolvedValue({ entity: story, sections: [] }); });

describe("CanonWorkspace", () => {
  it("shows every Story section as draft when it has no stored value", async () => { render(<CanonWorkspace projectRootPath="/projects/red-door" />); expect(await screen.findByRole("heading", { name: "Story Canon" })).toBeInTheDocument(); expect(screen.getByText("Premise")).toBeInTheDocument(); expect(screen.getByText("Active Skill Rules")).toBeInTheDocument(); expect(screen.getAllByText("DRAFT").length).toBe(7); });
  it("navigates to Characters without inventing data", async () => { const user = userEvent.setup(); render(<CanonWorkspace projectRootPath="/projects/red-door" />); await user.click(screen.getByRole("button", { name: "Characters" })); expect(await screen.findByRole("heading", { name: "Characters" })).toBeInTheDocument(); expect(screen.getByText("Select a character.")).toBeInTheDocument(); });
});
