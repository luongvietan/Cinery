import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ProviderCapabilities } from "@cinematic/domain";
import { WorldPlatePanel } from "./WorldPlatePanel";
import { createWorldPlateWorkflowRun } from "./api";
import { getAssetWithVersions } from "../assets/api";
import { advanceWorkflowRun } from "../workflows/api";
import { getProviderCapabilities, getProviderConfigurationStatus, listCustomProviders, listProviderModels, listProviders } from "../workflows/api";

vi.mock("./api", () => ({
  createWorldPlateWorkflowRun: vi.fn(),
}));
vi.mock("../assets/api", () => ({
  getAssetWithVersions: vi.fn(),
}));
vi.mock("../workflows/api", () => ({
  advanceWorkflowRun: vi.fn(),
  listProviders: vi.fn(),
  listCustomProviders: vi.fn(),
  getProviderCapabilities: vi.fn(),
  getProviderConfigurationStatus: vi.fn(),
  listProviderModels: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `mock-asset://${path}`,
}));

const openaiCapabilities: ProviderCapabilities = {
  mediaTypes: ["image"], supportsSeed: false, supportsNegativePrompt: false,
  supportsReferenceImage: true, supportsImageEdit: true, supportsMultipleReferenceImages: true,
  supportsImageToVideo: false, supportsCancel: false, supportsProgress: false,
  supportedAspectRatios: [], supportedModels: ["kira-3"],
};

const detail = {
  world: {
    id: "world-1", projectId: "project-1", canonLocationEntityId: "loc-1",
    worldPlateAssetId: "asset-1", createdAt: "now", updatedAt: "now",
  },
  location: {
    id: "loc-1", projectId: "project-1", type: "location" as const, name: "The Station",
    slug: "the-station", createdAt: "now", updatedAt: "now",
  },
  worldPlateAsset: {
    id: "asset-1", projectId: "project-1", type: "world_plate" as const, label: "THE-STATION-WORLD",
    ownerEntityId: "world-1", canonicalVersionId: null, createdAt: "now", updatedAt: "now",
  },
};

describe("WorldPlatePanel", () => {
  beforeEach(() => {
    vi.mocked(getAssetWithVersions).mockResolvedValue({ asset: detail.worldPlateAsset, versions: [] } as any);
    vi.mocked(listProviders).mockResolvedValue(["openai"]);
    vi.mocked(listCustomProviders).mockResolvedValue([]);
    vi.mocked(listProviderModels).mockResolvedValue(["kira-3"]);
    vi.mocked(getProviderCapabilities).mockResolvedValue(openaiCapabilities);
    vi.mocked(getProviderConfigurationStatus).mockResolvedValue({
      providerId: "openai", enabled: true, credentialConfigured: true, defaultModel: "kira-3", models: ["kira-3"],
    });
  });

  it("disables Generate Candidate until a provider and model are selected", async () => {
    vi.mocked(listProviders).mockResolvedValue([]);
    render(<WorldPlatePanel projectRootPath="/projects/red-door" detail={detail as any} />);
    const button = await screen.findByRole("button", { name: "Generate Candidate" });
    expect(button).toBeDisabled();
  });

  it("sends the selected providerId and modelId when generating a candidate", async () => {
    vi.mocked(createWorldPlateWorkflowRun).mockResolvedValue({ run: { id: "run-1" } } as any);
    vi.mocked(advanceWorkflowRun).mockResolvedValue({
      run: { id: "run-1", status: "waiting_for_approval", skillId: "world-builder", skillVersion: "1.0.0", operationId: "world.create_plate", inputJson: "{}", contextSnapshotJson: null, failureCode: null },
      steps: [],
      events: [],
      providerExecutions: [],
    } as any);
    const user = userEvent.setup();

    render(<WorldPlatePanel projectRootPath="/projects/red-door" detail={detail as any} />);

    const button = await screen.findByRole("button", { name: "Generate Candidate" });
    await waitFor(() => expect(button).toBeEnabled());

    await user.click(button);

    await waitFor(() => {
      expect(createWorldPlateWorkflowRun).toHaveBeenCalledWith(
        "/projects/red-door",
        "world-1",
        [],
        "openai",
        "kira-3",
      );
    });
  });
});
