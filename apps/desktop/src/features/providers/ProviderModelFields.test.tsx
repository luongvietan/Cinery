import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ProviderCapabilities, ProviderConfigurationStatus } from "@cinematic/domain";
import { describe, expect, it, vi, beforeEach } from "vitest";
import { ProviderModelFields } from "./ProviderModelFields";
import { getProviderCapabilities, getProviderConfigurationStatus, listProviderModels, listProviders } from "../workflows/api";

vi.mock("../workflows/api");

const openaiCapabilities: ProviderCapabilities = {
  mediaTypes: ["image"], supportsSeed: false, supportsNegativePrompt: false,
  supportsReferenceImage: true, supportsImageEdit: true, supportsMultipleReferenceImages: true,
  supportsImageToVideo: false, supportsCancel: false, supportsProgress: false,
  supportedAspectRatios: [], supportedModels: ["gpt-image-2"],
};
const mockCapabilities: ProviderCapabilities = {
  mediaTypes: ["image"], supportsSeed: true, supportsNegativePrompt: false,
  supportsReferenceImage: true, supportsImageEdit: true, supportsMultipleReferenceImages: true,
  supportsImageToVideo: false, supportsCancel: true, supportsProgress: true,
  supportedAspectRatios: [], supportedModels: ["mock-image-v1"],
};

function statusFor(providerId: string): ProviderConfigurationStatus {
  return {
    providerId,
    enabled: true,
    credentialConfigured: providerId === "mock",
    defaultModel: providerId === "openai" ? "gpt-image-2" : "mock-image-v1",
    models: providerId === "openai" ? ["gpt-image-2"] : ["mock-image-v1"],
  };
}

describe("ProviderModelFields", () => {
  beforeEach(() => {
    vi.mocked(listProviders).mockResolvedValue(["mock", "openai"]);
    vi.mocked(listProviderModels).mockImplementation(async (providerId) =>
      providerId === "openai" ? ["gpt-image-2"] : ["mock-image-v1"]);
    vi.mocked(getProviderCapabilities).mockImplementation(async (providerId) =>
      providerId === "openai" ? openaiCapabilities : mockCapabilities);
    vi.mocked(getProviderConfigurationStatus).mockImplementation(async (_root, providerId) => statusFor(providerId));
  });

  it("keeps OpenAI visible and clearly labelled when it is not configured", async () => {
    render(<ProviderModelFields projectRootPath="C:/p" value={{ providerId: "openai", modelId: "gpt-image-2" }} mediaType="image" requiresReferences onChange={vi.fn()} />);
    const providerSelect = (await screen.findByLabelText(/Provider/)) as HTMLSelectElement;
    const options = Array.from(providerSelect.options);
    expect(options.map((option) => option.value)).toContain("openai");
    expect(await screen.findByText("Credential not configured")).toBeInTheDocument();
  });

  it("defaults to the provider's default model and keeps the user's explicit selection", async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();
    render(<ProviderModelFields projectRootPath="C:/p" value={{ providerId: "openai", modelId: "gpt-image-2" }} mediaType="image" requiresReferences onChange={onChange} />);
    const modelSelect = await screen.findByLabelText(/Model/);
    expect(modelSelect).toHaveValue("gpt-image-2");
    await user.selectOptions(modelSelect, "gpt-image-2");
    expect(onChange).toHaveBeenCalledWith({ providerId: "openai", modelId: "gpt-image-2" });
  });

  it("disables providers that cannot satisfy reference-image requirements with a reason", async () => {
    vi.mocked(listProviders).mockResolvedValue(["mock", "openai", "mock-video"]);
    vi.mocked(getProviderCapabilities).mockImplementation(async (providerId) => {
      if (providerId === "mock-video") {
        return { ...mockCapabilities, mediaTypes: ["video" as const], supportedModels: ["mock-video-v1"] };
      }
      return providerId === "openai" ? openaiCapabilities : mockCapabilities;
    });
    render(<ProviderModelFields projectRootPath="C:/p" value={{ providerId: "openai", modelId: "gpt-image-2" }} mediaType="image" requiresReferences onChange={vi.fn()} />);
    const providerSelect = (await screen.findByLabelText(/Provider/)) as HTMLSelectElement;
    const videoOption = Array.from(providerSelect.options).find((option) => option.value === "mock-video");
    expect(videoOption).toBeDefined();
    expect(videoOption!.disabled).toBe(true);
    expect(videoOption!.textContent).toContain("does not support this media type");
  });

  it("exposes configuration status through an aria-describedby hook", async () => {
    render(<ProviderModelFields projectRootPath="C:/p" value={{ providerId: "openai", modelId: "gpt-image-2" }} mediaType="image" requiresReferences onChange={vi.fn()} />);
    const providerSelect = await screen.findByLabelText(/Provider/);
    expect(providerSelect).toHaveAccessibleDescription(/Credential not configured/);
  });
});
