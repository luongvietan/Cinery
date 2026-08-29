import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProviderSettings } from "./ProviderSettings";
import { deleteCustomProvider, listCustomProviders, testCustomProviderConnection, upsertCustomProvider } from "../workflows/api";

vi.mock("../workflows/api");

describe("ProviderSettings", () => {
  beforeEach(() => {
    vi.mocked(listCustomProviders).mockResolvedValue([]);
    vi.mocked(upsertCustomProvider).mockImplementation(async (_root, definition) => ({ ...definition, apiKey: undefined, apiKeyHint: definition.apiKey ? "sk-j9ml•••ray" : null, headers: definition.headers.map((header) => ({ name: header.name })) }));
    vi.mocked(deleteCustomProvider).mockResolvedValue();
    vi.mocked(testCustomProviderConnection).mockResolvedValue({ providerId: "video_provider", endpoint: "https://video.example.test/v1/models", connected: true, statusCode: 200, message: "Endpoint reachable and credentials were not rejected; no inference was run." });
  });

  it("shows only custom provider management and no built-in provider selector", async () => {
    render(<ProviderSettings projectRootPath="C:/projects/red-door" />);
    expect(await screen.findByRole("heading", { name: "Custom providers" })).toBeInTheDocument();
    expect(screen.queryByLabelText("Provider", { selector: "select" })).not.toBeInTheDocument();
    expect(screen.getByLabelText("Purpose")).toHaveValue("image");
  });

  it("saves a provider purpose, models, API key, and headers", async () => {
    const user = userEvent.setup();
    render(<ProviderSettings projectRootPath="C:/projects/red-door" />);
    await user.selectOptions(screen.getByLabelText("Purpose"), "video");
    await user.type(screen.getByLabelText("Provider ID"), "video_provider");
    await user.type(screen.getByLabelText("Display name"), "Video Provider");
    await user.type(screen.getByLabelText("Base URL"), "https://video.example.test/v1");
    await user.type(screen.getByLabelText("API key (optional)"), "video-secret");
    await user.type(screen.getByLabelText("Model ID"), "video-v1");
    await user.type(screen.getByLabelText("Model name"), "Video V1");
    await user.click(screen.getByRole("button", { name: "Add header" }));
    await user.type(screen.getByLabelText("Header"), "X-Workspace");
    await user.type(screen.getByLabelText("Value"), "workspace-secret");
    await user.click(screen.getByRole("button", { name: "Save provider" }));
    await waitFor(() => expect(upsertCustomProvider).toHaveBeenCalledWith("C:/projects/red-door", expect.objectContaining({ providerId: "video_provider", purpose: "video", apiKey: "video-secret", headers: [{ name: "X-Workspace", value: "workspace-secret" }] })));
    expect(screen.queryByText("video-secret")).not.toBeInTheDocument();
  });

  it("shows the masked vault hint after saving and when selecting a saved provider", async () => {
    const user = userEvent.setup();
    render(<ProviderSettings projectRootPath="C:/projects/red-door" />);
    await user.type(screen.getByLabelText("Provider ID"), "video_provider");
    await user.type(screen.getByLabelText("Display name"), "Video Provider");
    await user.type(screen.getByLabelText("Base URL"), "https://video.example.test/v1");
    await user.type(screen.getByLabelText("API key (optional)"), "sk-j9mlQwErTyXzray");
    await user.type(screen.getByLabelText("Model ID"), "video-v1");
    await user.type(screen.getByLabelText("Model name"), "Video V1");
    await user.click(screen.getByRole("button", { name: "Save provider" }));

    expect(await screen.findByText("sk-j9ml•••ray")).toBeInTheDocument();
    expect(screen.getByLabelText("API key (optional)")).toHaveAttribute("placeholder", "Stored in vault: sk-j9ml•••ray");
    expect(screen.queryByText("sk-j9mlQwErTyXzray")).not.toBeInTheDocument();

    vi.mocked(listCustomProviders).mockResolvedValue([{ providerId: "video_provider", displayName: "Video Provider", baseUrl: "https://video.example.test/v1", purpose: "video", apiKeyHint: "sk-j9ml•••ray", models: [{ id: "video-v1", name: "Video V1" }], headers: [] }]);
    cleanup();
    render(<ProviderSettings projectRootPath="C:/projects/red-door" />);
    await user.selectOptions(await screen.findByLabelText("Saved providers"), "video_provider");
    const hint = await screen.findByText((_, element) => element?.textContent === "Stored credential: sk-j9ml•••ray — leave the field empty to keep it." && element.tagName === "P");
    expect(hint).toBeInTheDocument();
    expect(screen.getByLabelText("API key (optional)")).toHaveAttribute("placeholder", "Stored in vault: sk-j9ml•••ray");
  });

  it("tests a saved provider without invoking a generation endpoint", async () => {
    vi.mocked(listCustomProviders).mockResolvedValue([{ providerId: "video_provider", displayName: "Video Provider", baseUrl: "https://video.example.test/v1", purpose: "video", models: [{ id: "video-v1", name: "Video V1" }], headers: [] }]);
    const user = userEvent.setup();
    render(<ProviderSettings projectRootPath="C:/projects/red-door" />);
    await user.selectOptions(await screen.findByLabelText("Saved providers"), "video_provider");
    await user.click(screen.getByRole("button", { name: "Test connection" }));
    await waitFor(() => expect(testCustomProviderConnection).toHaveBeenCalledWith("C:/projects/red-door", "video_provider"));
    expect(await screen.findByText("Endpoint reachable and credentials were not rejected; no inference was run.")).toBeInTheDocument();
  });

  it("ignores a connection result after the user switches providers", async () => {
    const providers = [
      { providerId: "first", displayName: "First", baseUrl: "https://first.example.test/v1", purpose: "llm" as const, models: [{ id: "m1", name: "M1" }], headers: [] },
      { providerId: "second", displayName: "Second", baseUrl: "https://second.example.test/v1", purpose: "llm" as const, models: [{ id: "m2", name: "M2" }], headers: [] },
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

    await act(async () => resolveProbe({ providerId: "first", endpoint: "https://first.example.test/v1/models", connected: true, statusCode: 200, message: "stale success" }));

    expect(screen.queryByText("stale success")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Test connection" })).toBeEnabled();
  });

  it("clears testing state when save supersedes a pending probe", async () => {
    const provider = { providerId: "first", displayName: "First", baseUrl: "https://first.example.test/v1", purpose: "llm" as const, models: [{ id: "m1", name: "M1" }], headers: [] };
    vi.mocked(listCustomProviders).mockResolvedValue([provider]);
    let resolveProbe!: (value: Awaited<ReturnType<typeof testCustomProviderConnection>>) => void;
    vi.mocked(testCustomProviderConnection).mockReturnValue(new Promise((resolve) => { resolveProbe = resolve; }));
    const user = userEvent.setup();
    render(<ProviderSettings projectRootPath="C:/projects/red-door" />);
    await user.selectOptions(await screen.findByLabelText("Saved providers"), "first");
    await user.click(screen.getByRole("button", { name: "Test connection" }));
    expect(screen.getByRole("button", { name: "Testing…" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "Save provider" }));
    expect(await screen.findByRole("button", { name: "Test connection" })).toBeEnabled();
    await act(async () => resolveProbe({ providerId: "first", endpoint: "https://first.example.test/v1/models", connected: true, statusCode: 200, message: "stale success" }));
    expect(screen.queryByText("stale success")).not.toBeInTheDocument();
  });
});
