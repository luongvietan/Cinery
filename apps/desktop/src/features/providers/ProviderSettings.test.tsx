import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProviderSettings } from "./ProviderSettings";
import { configureProvider, getProviderCapabilities, getProviderConfigurationStatus, listProviderModels, listProviders, removeProviderCredentials, validateProviderConfiguration } from "../workflows/api";

vi.mock("../workflows/api");

describe("ProviderSettings", () => {
  beforeEach(() => {
    vi.mocked(listProviders).mockResolvedValue(["mock"]);
    vi.mocked(listProviderModels).mockResolvedValue(["mock-image-v1"]);
    vi.mocked(getProviderCapabilities).mockResolvedValue({ mediaTypes: ["image"], supportsSeed: true, supportsNegativePrompt: false, supportsReferenceImage: true, supportsMultipleReferenceImages: true, supportsImageToVideo: false, supportsCancel: true, supportsProgress: true, supportedAspectRatios: [], supportedModels: ["mock-image-v1"] });
    vi.mocked(getProviderConfigurationStatus).mockResolvedValue({ providerId: "mock", enabled: true, credentialConfigured: true, credentialReference: null, defaultModel: "mock-image-v1" });
    vi.mocked(configureProvider).mockResolvedValue({ providerId: "mock", enabled: true, credentialConfigured: true, credentialReference: null, defaultModel: "mock-image-v1" });
    vi.mocked(removeProviderCredentials).mockResolvedValue();
    vi.mocked(validateProviderConfiguration).mockResolvedValue();
  });

  it("keeps credential input masked and exposes accessible provider configuration controls", async () => {
    render(<ProviderSettings projectRootPath="C:/projects/red-door" />);
    expect(await screen.findByLabelText("Provider")).toHaveValue("mock");
    expect(screen.getByLabelText("Credential environment variable")).toHaveAttribute("type", "password");
    expect(screen.getByRole("button", { name: "Save configuration" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Validate" })).toBeEnabled();
  });
});
