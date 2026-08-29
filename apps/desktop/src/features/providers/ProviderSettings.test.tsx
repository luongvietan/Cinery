import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProviderSettings } from "./ProviderSettings";
import {
  deleteCustomProvider,
  listCustomProviders,
  listProviderPresets,
  testCustomProviderConnection,
  upsertCustomProvider,
} from "../workflows/api";
import type { CustomProviderDefinition, ProviderPreset } from "@cinematic/domain";

vi.mock("../workflows/api");

const openAiCompatiblePreset: ProviderPreset = {
  id: "openai-compatible",
  label: "OpenAI Compatible",
  description: "Any service speaking the OpenAI images API.",
  internal: false,
  defaultBaseUrl: "https://api.openai.com/v1",
  requiresAccountId: false,
  auth: { mode: "bearer", credentialName: null },
  defaultModels: [["gpt-image-2", "GPT Image 2"]],
  runtime: {
    auth: { mode: "bearer", credentialName: null },
    headers: {},
    operations: {
      "image.generate": {
        method: "POST",
        pathTemplate: "/images/generations",
        requestType: "json",
        requestMapping: { model: "{{model}}", prompt: "{{prompt}}", size: "1024x1024" },
        headers: {},
        response: {
          outputsPath: "data",
          urlPath: "url",
          base64Path: "b64_json",
          binaryResponse: false,
          mimeType: "image/png",
          filename: "generated.png",
        },
        job: null,
      },
      validate: {
        method: "GET",
        pathTemplate: "/models",
        requestType: "json",
        headers: {},
        response: { binaryResponse: true, mimeType: "image/png", filename: "generated.png" },
        job: null,
      },
    },
  },
};

const cloudflarePreset: ProviderPreset = {
  id: "cloudflare-workers-ai",
  label: "Cloudflare Workers AI",
  description: "Run image models like FLUX.1 Schnell on Cloudflare.",
  internal: false,
  defaultBaseUrl: "https://api.cloudflare.com/client/v4/accounts/{accountId}/ai/run",
  requiresAccountId: true,
  auth: { mode: "bearer", credentialName: null },
  defaultModels: [["@cf/black-forest-labs/flux-1-schnell", "FLUX.1 Schnell"]],
  runtime: {
    auth: { mode: "bearer", credentialName: null },
    accountId: null,
    headers: {},
    operations: {
      "image.generate": {
        method: "POST",
        pathTemplate: "/{model}",
        requestType: "json",
        requestMapping: { prompt: "{{prompt}}", steps: "{{steps}}" },
        headers: {},
        response: { base64Path: "result.image", binaryResponse: false, mimeType: "image/png", filename: "generated.png" },
        job: null,
      },
      validate: {
        method: "POST",
        pathTemplate: "/{model}",
        requestType: "json",
        requestMapping: { prompt: "simple provider validation test", steps: 1 },
        headers: {},
        response: { binaryResponse: true, mimeType: "image/png", filename: "generated.png" },
        job: null,
      },
    },
  },
};

const customRestPreset: ProviderPreset = {
  id: "custom",
  label: "Custom REST API",
  description: "Connect any HTTP endpoint.",
  internal: false,
  defaultBaseUrl: "",
  requiresAccountId: false,
  auth: { mode: "bearer", credentialName: null },
  defaultModels: [["default", "Default model"]],
  runtime: {
    auth: { mode: "bearer", credentialName: null },
    headers: {},
    operations: {},
  },
};

function savedProvider(overrides: Partial<CustomProviderDefinition> = {}): CustomProviderDefinition {
  return {
    providerId: "video_provider",
    displayName: "Video Provider",
    baseUrl: "https://video.example.test/v1",
    purpose: "video",
    presetId: null,
    runtime: { auth: { mode: "bearer", credentialName: null }, headers: {}, operations: {} },
    apiKeyHint: null,
    models: [{ id: "video-v1", name: "Video V1" }],
    headers: [],
    ...overrides,
  };
}

describe("ProviderSettings", () => {
  beforeEach(() => {
    vi.mocked(listCustomProviders).mockResolvedValue([]);
    vi.mocked(listProviderPresets).mockResolvedValue([openAiCompatiblePreset, cloudflarePreset, customRestPreset]);
    vi.mocked(upsertCustomProvider).mockImplementation(async (_root, definition) => ({
      ...definition,
      apiKey: undefined,
      apiKeyHint: definition.apiKey ? "sk-j9ml•••ray" : null,
      headers: definition.headers.map((header) => ({ name: header.name })),
    }));
    vi.mocked(deleteCustomProvider).mockResolvedValue();
    vi.mocked(testCustomProviderConnection).mockResolvedValue({
      providerId: "cloudflare",
      endpoint: "https://api.cloudflare.com/client/v4/accounts/acc/ai/run/@cf/model",
      connected: true,
      statusCode: 200,
      message: "Endpoint reachable and credentials were not rejected; no inference was run.",
    });
  });

  it("offers preset connection types and hides the built-in provider selector", async () => {
    render(<ProviderSettings projectRootPath="C:/projects/red-door" />);
    expect(await screen.findByRole("heading", { name: "Connect an AI service" })).toBeInTheDocument();
    expect(screen.queryByLabelText("Provider", { selector: "select" })).not.toBeInTheDocument();
    expect(await screen.findByRole("radio", { name: /Cloudflare Workers AI/ })).toBeInTheDocument();
    // No implementation jargon in simple mode.
    expect(screen.queryByText(/path template/i)).not.toBeInTheDocument();
    // The default preset pre-fills the base URL.
    expect(await screen.findByRole("textbox", { name: /Base URL/ })).toHaveValue("https://api.openai.com/v1");
  });

  it("configures Cloudflare with account ID, token, and model in simple mode", async () => {
    const user = userEvent.setup();
    render(<ProviderSettings projectRootPath="C:/projects/red-door" />);
    await user.click(await screen.findByRole("radio", { name: /Cloudflare Workers AI/ }));

    expect(await screen.findByRole("textbox", { name: /Account ID/ })).toBeInTheDocument();
    await user.type(screen.getByLabelText("Display name"), "My Cloudflare");
    await user.type(screen.getByRole("textbox", { name: /Account ID/ }), "acc-123");
    expect(screen.getByRole("textbox", { name: /Base URL/ })).toHaveValue(
      "https://api.cloudflare.com/client/v4/accounts/{accountId}/ai/run",
    );
    await user.type(screen.getByLabelText("API token"), "cf-secret-token");
    expect(screen.getByLabelText("Model ID")).toHaveValue("@cf/black-forest-labs/flux-1-schnell");
    await user.click(screen.getByRole("button", { name: "Save provider" }));

    await waitFor(() =>
      expect(upsertCustomProvider).toHaveBeenCalledWith(
        "C:/projects/red-door",
        expect.objectContaining({
          purpose: "image",
          presetId: "cloudflare-workers-ai",
          baseUrl: "https://api.cloudflare.com/client/v4/accounts/{accountId}/ai/run",
          apiKey: "cf-secret-token",
          runtime: expect.objectContaining({
            accountId: "acc-123",
            operations: expect.objectContaining({ "image.generate": expect.anything() }),
          }),
        }),
      ),
    );
    expect(screen.queryByText("cf-secret-token")).not.toBeInTheDocument();
  });

  it("guides a custom REST API endpoint without exposing jargon", async () => {
    const user = userEvent.setup();
    render(<ProviderSettings projectRootPath="C:/projects/red-door" />);
    await user.click(await screen.findByRole("radio", { name: /Custom REST API/ }));

    await user.type(screen.getByLabelText("Display name"), "My API");
    await user.type(screen.getByRole("textbox", { name: /Base URL/ }), "https://example.com/api");
    const requestPath = screen.getByRole("textbox", { name: /Request path/ });
    await user.clear(requestPath);
    await user.type(requestPath, "/v2/render");
    await user.selectOptions(screen.getByLabelText("Output type"), "image-base64");
    const responsePath = screen.getByRole("textbox", { name: /Response path/ });
    await user.clear(responsePath);
    await user.type(responsePath, "output.image");
    await user.type(screen.getByLabelText("API key (optional)"), "sk-custom");
    await user.click(screen.getByRole("button", { name: "Save provider" }));

    await waitFor(() =>
      expect(upsertCustomProvider).toHaveBeenCalledWith(
        "C:/projects/red-door",
        expect.objectContaining({
          purpose: "image",
          baseUrl: "https://example.com/api",
          runtime: expect.objectContaining({
            operations: expect.objectContaining({
              "image.generate": expect.objectContaining({
                method: "POST",
                pathTemplate: "/v2/render",
                response: expect.objectContaining({ base64Path: "output.image" }),
              }),
            }),
          }),
        }),
      ),
    );
  });

  it("edits the declarative operations JSON under advanced settings", async () => {
    const user = userEvent.setup();
    render(<ProviderSettings projectRootPath="C:/projects/red-door" />);
    await user.click(await screen.findByRole("button", { name: "Advanced settings" }));
    const operations = await screen.findByLabelText(/Operations \(JSON\)/);
    await waitFor(() => expect((operations as HTMLTextAreaElement).value).toContain("image.generate"));
    const edited = JSON.stringify({
      "image.generate": {
        method: "PUT",
        pathTemplate: "/gen",
        requestType: "json",
        requestMapping: { prompt: "{{prompt}}" },
        response: { urlPath: "out", binaryResponse: false, mimeType: "image/png", filename: "generated.png" },
      },
    });
    fireEvent.change(operations, { target: { value: edited } });
    fireEvent.blur(operations);
    await user.type(screen.getByLabelText("Display name"), "Advanced");
    await user.click(screen.getByRole("button", { name: "Save provider" }));

    await waitFor(() =>
      expect(upsertCustomProvider).toHaveBeenCalledWith(
        "C:/projects/red-door",
        expect.objectContaining({
          runtime: expect.objectContaining({
            operations: expect.objectContaining({
              "image.generate": expect.objectContaining({ method: "PUT", pathTemplate: "/gen" }),
            }),
          }),
        }),
      ),
    );
  });

  it("shows the masked vault hint after saving and when selecting a saved provider", async () => {
    const user = userEvent.setup();
    render(<ProviderSettings projectRootPath="C:/projects/red-door" />);
    await user.type(await screen.findByLabelText("Display name"), "Video Provider");
    await user.type(screen.getByRole("textbox", { name: /Base URL/ }), "https://video.example.test/v1");
    await user.type(screen.getByLabelText("API key (optional)"), "sk-j9mlQwErTyXzray");
    await user.type(screen.getByLabelText("Model ID"), "video-v1");
    await user.type(screen.getByLabelText("Model name"), "Video V1");
    await user.click(screen.getByRole("button", { name: "Save provider" }));

    expect(await screen.findByText("sk-j9ml•••ray")).toBeInTheDocument();
    expect(screen.getByLabelText("API key (optional)")).toHaveAttribute("placeholder", "Stored in vault: sk-j9ml•••ray");
    expect(screen.queryByText("sk-j9mlQwErTyXzray")).not.toBeInTheDocument();

    vi.mocked(listCustomProviders).mockResolvedValue([
      savedProvider({ apiKeyHint: "sk-j9ml•••ray" }),
    ]);
    cleanup();
    render(<ProviderSettings projectRootPath="C:/projects/red-door" />);
    await user.selectOptions(await screen.findByLabelText("Saved providers"), "video_provider");
    const hint = await screen.findByText(
      (_, element) => element?.textContent === "Stored credential: sk-j9ml•••ray — leave the field empty to keep it." && element.tagName === "P",
    );
    expect(hint).toBeInTheDocument();
    expect(screen.getByLabelText("API key (optional)")).toHaveAttribute("placeholder", "Stored in vault: sk-j9ml•••ray");
  });

  it("tests a saved provider without invoking a generation endpoint", async () => {
    vi.mocked(listCustomProviders).mockResolvedValue([savedProvider()]);
    const user = userEvent.setup();
    render(<ProviderSettings projectRootPath="C:/projects/red-door" />);
    await user.selectOptions(await screen.findByLabelText("Saved providers"), "video_provider");
    await user.click(screen.getByRole("button", { name: "Test connection" }));
    await waitFor(() => expect(testCustomProviderConnection).toHaveBeenCalledWith("C:/projects/red-door", "video_provider"));
    expect(await screen.findByText("Endpoint reachable and credentials were not rejected; no inference was run.")).toBeInTheDocument();
  });

  it("ignores a connection result after the user switches providers", async () => {
    const providers = [
      savedProvider({ providerId: "first", displayName: "First", baseUrl: "https://first.example.test/v1", models: [{ id: "m1", name: "M1" }] }),
      savedProvider({ providerId: "second", displayName: "Second", baseUrl: "https://second.example.test/v1", models: [{ id: "m2", name: "M2" }] }),
    ];
    vi.mocked(listCustomProviders).mockResolvedValue(providers);
    let resolveProbe!: (value: Awaited<ReturnType<typeof testCustomProviderConnection>>) => void;
    vi.mocked(testCustomProviderConnection).mockReturnValue(new Promise((resolve) => { resolveProbe = resolve; }));
    const user = userEvent.setup();
    render(<ProviderSettings projectRootPath="C:/projects/red-door" />);
    const saved = await screen.findByLabelText("Saved providers");
    await user.selectOptions(saved, "first");
    await user.click(screen.getByRole("button", { name: "Test connection" }));
    await user.selectOptions(saved, "second");

    await act(async () =>
      resolveProbe({ providerId: "first", endpoint: "https://first.example.test/v1/models", connected: true, statusCode: 200, message: "stale success" }),
    );

    expect(screen.queryByText("stale success")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Test connection" })).toBeEnabled();
  });

  it("clears testing state when save supersedes a pending probe", async () => {
    vi.mocked(listCustomProviders).mockResolvedValue([savedProvider()]);
    let resolveProbe!: (value: Awaited<ReturnType<typeof testCustomProviderConnection>>) => void;
    vi.mocked(testCustomProviderConnection).mockReturnValue(new Promise((resolve) => { resolveProbe = resolve; }));
    const user = userEvent.setup();
    render(<ProviderSettings projectRootPath="C:/projects/red-door" />);
    await user.selectOptions(await screen.findByLabelText("Saved providers"), "video_provider");
    await user.click(screen.getByRole("button", { name: "Test connection" }));
    expect(screen.getByRole("button", { name: "Testing…" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "Save provider" }));
    expect(await screen.findByRole("button", { name: "Test connection" })).toBeEnabled();
    await act(async () =>
      resolveProbe({ providerId: "video_provider", endpoint: "https://video.example.test/v1/models", connected: true, statusCode: 200, message: "stale success" }),
    );
    expect(screen.queryByText("stale success")).not.toBeInTheDocument();
  });
});
