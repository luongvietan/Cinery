import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ProviderCapabilities, ProviderConfigurationStatus } from "@cinematic/domain";
import { describe, expect, it, vi, beforeEach } from "vitest";
import { ProviderModelFields } from "./ProviderModelFields";
import { getProviderCapabilities, getProviderConfigurationStatus, listCustomProviders, listProviderModels, listProviders } from "../workflows/api";

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
    vi.mocked(listCustomProviders).mockResolvedValue([]);
    vi.mocked(listProviders).mockResolvedValue(["mock", "openai"]);
    vi.mocked(listProviderModels).mockImplementation(async (providerId) =>
      providerId === "openai" ? ["gpt-image-2"] : ["mock-image-v1"]);
    vi.mocked(getProviderCapabilities).mockImplementation(async (providerId) =>
      providerId === "openai" ? openaiCapabilities : mockCapabilities);
    vi.mocked(getProviderConfigurationStatus).mockImplementation(async (_root, providerId) => statusFor(providerId));
  });

  it("keeps OpenAI visible and clearly labelled when it is not configured", async () => {
    render(<ProviderModelFields projectRootPath="C:/p" value={{ providerId: "openai", modelId: "gpt-image-2" }} mediaType="image" requiresReferences onChange={vi.fn()} />);
    const providerSelect = (await screen.findByLabelText(/AI service/)) as HTMLSelectElement;
    const options = Array.from(providerSelect.options);
    expect(options.map((option) => option.value)).toContain("openai");
    expect(await screen.findByText("This service needs its API key before it can generate anything")).toBeInTheDocument();
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
    const providerSelect = (await screen.findByLabelText(/AI service/)) as HTMLSelectElement;
    const videoOption = Array.from(providerSelect.options).find((option) => option.value === "mock-video");
    expect(videoOption).toBeDefined();
    expect(videoOption!.disabled).toBe(true);
    expect(videoOption!.textContent).toContain("does not support this media type");
  });

  it("exposes configuration status through an aria-describedby hook", async () => {
    render(<ProviderModelFields projectRootPath="C:/p" value={{ providerId: "openai", modelId: "gpt-image-2" }} mediaType="image" requiresReferences onChange={vi.fn()} />);
    const providerSelect = await screen.findByLabelText(/AI service/);
    expect(providerSelect).toHaveAccessibleDescription(/needs its API key/);
  });

  it("hides the local mock and dry-run providers from the picker", async () => {
    render(<ProviderModelFields projectRootPath="C:/p" value={{ providerId: "", modelId: "" }} mediaType="image" requiresReferences={false} onChange={vi.fn()} />);
    const providerSelect = (await screen.findByLabelText(/AI service/)) as HTMLSelectElement;
    const values = Array.from(providerSelect.options).map((option) => option.value);
    expect(values).not.toContain("mock");
    expect(values).not.toContain("dry_run");
    expect(values).toContain("openai");
  });

  it("auto-selects the first configured compatible custom provider when nothing is chosen", async () => {
    vi.mocked(listProviders).mockResolvedValue(["my-studio", "unconfigured-one"]);
    vi.mocked(listCustomProviders).mockResolvedValue([
      {
        providerId: "my-studio", displayName: "My Studio", baseUrl: "https://api.my-studio.test/v1",
        purpose: "image", models: [{ id: "studio-image-1", name: "Studio Image 1" }], headers: [],
      },
      {
        providerId: "unconfigured-one", displayName: "Unconfigured", baseUrl: "https://api.other.test/v1",
        purpose: "image", models: [{ id: "other-1", name: "Other 1" }], headers: [],
      },
    ]);
    vi.mocked(getProviderCapabilities).mockResolvedValue(openaiCapabilities);
    vi.mocked(listProviderModels).mockImplementation(async (providerId) =>
      providerId === "my-studio" ? ["studio-image-1"] : ["other-1"]);
    vi.mocked(getProviderConfigurationStatus).mockImplementation(async (_root, providerId) => ({
      providerId,
      enabled: true,
      credentialConfigured: providerId === "my-studio",
      defaultModel: providerId === "my-studio" ? "studio-image-1" : "other-1",
      models: providerId === "my-studio" ? ["studio-image-1"] : ["other-1"],
    }));

    const onChange = vi.fn();
    render(<ProviderModelFields projectRootPath="C:/p" value={{ providerId: "", modelId: "" }} mediaType="image" requiresReferences={false} onChange={onChange} />);

    await waitFor(() => {
      expect(onChange).toHaveBeenCalledWith({ providerId: "my-studio", modelId: "studio-image-1" });
    });
  });

  it("does not offer an LLM-only custom provider to image workflows", async () => {
    vi.mocked(listProviders).mockResolvedValue(["llm_only"]);
    vi.mocked(listCustomProviders).mockResolvedValue([{
      providerId: "llm_only", displayName: "LLM only", baseUrl: "https://api.example.test/v1",
      purpose: "llm", models: [{ id: "chat-v1", name: "Chat V1" }], headers: [],
    }]);
    vi.mocked(listProviderModels).mockResolvedValue(["chat-v1"]);
    vi.mocked(getProviderCapabilities).mockRejectedValue(new Error("custom provider"));
    vi.mocked(getProviderConfigurationStatus).mockResolvedValue({
      providerId: "llm_only", enabled: true, credentialConfigured: true,
      defaultModel: "chat-v1", models: ["chat-v1"],
    });

    render(<ProviderModelFields projectRootPath="C:/p" value={{ providerId: "", modelId: "" }} mediaType="image" requiresReferences={false} onChange={vi.fn()} />);

    const option = Array.from(((await screen.findByLabelText(/AI service/)) as HTMLSelectElement).options)[0];
    expect(option.value).toBe("llm_only");
    expect(option.disabled).toBe(true);
    expect(option.textContent).toContain("not compatible");
  });

  it("offers only providers and models that advertise video.imageToVideo", async () => {
    vi.mocked(listProviders).mockResolvedValue(["plain-video", "i2v"]);
    vi.mocked(getProviderCapabilities).mockImplementation(async (providerId) =>
      providerId === "plain-video"
        ? { ...mockCapabilities, mediaTypes: ["video" as const], supportsImageToVideo: false, supportedModels: ["plain-v1"] }
        : { ...mockCapabilities, mediaTypes: ["video" as const], supportsImageToVideo: true, supportedModels: [] },
    );
    vi.mocked(listProviderModels).mockImplementation(async (providerId) =>
      providerId === "plain-video" ? ["plain-v1"] : ["text-v1", "motion-v1"]);
    vi.mocked(listCustomProviders).mockResolvedValue([
      {
        providerId: "i2v", displayName: "I2V", baseUrl: "https://api.i2v.test/v1",
        purpose: "video", models: [
          { id: "text-v1", name: "Text", capabilities: ["video.generate"] },
          { id: "motion-v1", name: "Motion", capabilities: ["video.imageToVideo"] },
        ],
        headers: [],
      },
    ]);
    vi.mocked(getProviderConfigurationStatus).mockImplementation(async (_root, providerId) => ({
      providerId,
      enabled: true,
      credentialConfigured: true,
      defaultModel: providerId === "plain-video" ? "plain-v1" : "motion-v1",
      models: providerId === "plain-video" ? ["plain-v1"] : ["text-v1", "motion-v1"],
    }));

    render(
      <ProviderModelFields
        projectRootPath="C:/p"
        value={{ providerId: "i2v", modelId: "motion-v1" }}
        mediaType="video"
        requiresReferences={false}
        requiredOperation="video.imageToVideo"
        onChange={vi.fn()}
      />,
    );
    expect(await screen.findByRole("option", { name: /i2v/ })).toBeEnabled();
    expect(screen.getByRole("option", { name: /plain-video/ })).toBeDisabled();
    expect(screen.getByRole("option", { name: "motion-v1" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "text-v1" })).not.toBeInTheDocument();
  });
});
