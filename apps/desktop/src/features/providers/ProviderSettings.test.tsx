import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ProviderCapabilities, ProviderConfigurationStatus } from "@cinematic/domain";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProviderSettings } from "./ProviderSettings";
import { configureProvider, getProviderCapabilities, getProviderConfigurationStatus, listProviderModels, listProviders, removeProviderCredentials, saveProviderCredential, validateProviderConfiguration } from "../workflows/api";

vi.mock("../workflows/api");

const capabilities: ProviderCapabilities = { mediaTypes: ["image"], supportsSeed: true, supportsNegativePrompt: false, supportsReferenceImage: true, supportsImageEdit: true, supportsMultipleReferenceImages: true, supportsImageToVideo: false, supportsCancel: true, supportsProgress: true, supportedAspectRatios: [], supportedModels: ["gpt-image-2"] };

function statusFor(providerId: string, configured: boolean): ProviderConfigurationStatus {
  return { providerId, enabled: true, credentialConfigured: configured, defaultModel: "gpt-image-2", models: ["gpt-image-2"] };
}

describe("ProviderSettings", () => {
  beforeEach(() => {
    vi.mocked(listProviders).mockResolvedValue(["mock", "openai"]);
    vi.mocked(listProviderModels).mockImplementation(async (providerId: string) => providerId === "openai" ? ["gpt-image-2"] : ["mock-image-v1"]);
    vi.mocked(getProviderCapabilities).mockResolvedValue(capabilities);
    vi.mocked(configureProvider).mockImplementation(async (_root: string, config: Record<string, unknown>) => statusFor(String(config.providerId), true));
    vi.mocked(saveProviderCredential).mockImplementation(async (_root: string, providerId: string) => statusFor(providerId, true));
    vi.mocked(removeProviderCredentials).mockResolvedValue();
    vi.mocked(validateProviderConfiguration).mockResolvedValue();
  });

  it("keeps the credential write-only and exposes accessible provider controls", async () => {
    vi.mocked(getProviderConfigurationStatus).mockResolvedValue(statusFor("openai", true));
    const user = userEvent.setup();
    render(<ProviderSettings projectRootPath="C:/projects/red-door" />);

    await screen.findByLabelText("Provider");
    await user.selectOptions(screen.getByLabelText("Provider"), "openai");

    const secret = await screen.findByLabelText("API key", { selector: "input" });
    expect(secret).toHaveAttribute("type", "password");
    expect(screen.getByText("Credential configured")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save configuration" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Validate" })).toBeEnabled();

    // Saving a replacement key clears the field and never displays the secret.
    await user.type(secret, "sk-super-secret-value");
    await user.click(screen.getByRole("button", { name: "Save credential" }));
    await waitFor(() => expect(saveProviderCredential).toHaveBeenCalledWith("C:/projects/red-door", "openai", "sk-super-secret-value", "gpt-image-2"));
    expect(await screen.findByText("Credential saved to the operating system credential vault.")).toBeInTheDocument();
    expect(secret).toHaveValue("");
    expect(screen.queryByText(/sk-super-secret-value/)).not.toBeInTheDocument();
  });

  it("shows not-configured status for a provider without a credential", async () => {
    vi.mocked(getProviderConfigurationStatus).mockResolvedValue(statusFor("openai", false));
    const user = userEvent.setup();
    render(<ProviderSettings projectRootPath="C:/projects/red-door" />);

    await screen.findByLabelText("Provider");
    await user.selectOptions(screen.getByLabelText("Provider"), "openai");

    expect(await screen.findByText("Credential not configured")).toBeInTheDocument();
    expect(screen.getByLabelText("API key", { selector: "input" })).toHaveValue("");
  });

  it("does not offer a credential field for local providers", async () => {
    vi.mocked(getProviderConfigurationStatus).mockResolvedValue({ providerId: "mock", enabled: true, credentialConfigured: true, defaultModel: "mock-image-v1", models: ["mock-image-v1"], } as ProviderConfigurationStatus);
    render(<ProviderSettings projectRootPath="C:/projects/red-door" />);
    expect(await screen.findByText("This provider runs locally and needs no credential.")).toBeInTheDocument();
    expect(screen.queryByLabelText("API key")).not.toBeInTheDocument();
  });
});
