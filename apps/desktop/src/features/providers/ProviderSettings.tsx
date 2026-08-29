import { useEffect, useRef, useState } from "react";
import type {
  CustomProviderDefinition,
  ProviderAuthMode,
  ProviderPreset,
} from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import {
  deleteCustomProvider,
  listCustomProviders,
  listProviderPresets,
  testCustomProviderConnection,
  upsertCustomProvider,
} from "../workflows/api";

/** Synthetic preset for text (LLM) services, which stay purpose-based. */
const LLM_PRESET: ProviderPreset = {
  id: "llm",
  label: "Text AI (LLM)",
  description:
    "A chat-completions text service used for suggestions. Needs the service's base URL and API key.",
  internal: false,
  defaultBaseUrl: "https://api.openai.com/v1",
  requiresAccountId: false,
  auth: { mode: "bearer", credentialName: null },
  defaultModels: [],
  runtime: { auth: { mode: "bearer", credentialName: null }, headers: {}, operations: {} },
};

const emptyProvider = (): CustomProviderDefinition => ({
  providerId: "",
  displayName: "",
  baseUrl: "",
  purpose: "image",
  presetId: null,
  runtime: { auth: { mode: "bearer", credentialName: null }, headers: {}, operations: {} },
  models: [{ id: "", name: "" }],
  headers: [],
});

function derivedPurpose(preset: ProviderPreset | null): CustomProviderDefinition["purpose"] {
  if (!preset) return "image";
  if (preset.id === "llm") return "llm";
  const operations = preset.runtime.operations ?? {};
  if ("video.generate" in operations || "video.imageToVideo" in operations) return "video";
  return "image";
}

function uniqueProviderId(base: string, existing: string[]): string {
  let candidate = base;
  let counter = 2;
  while (existing.includes(candidate)) {
    candidate = `${base}-${counter}`;
    counter += 1;
  }
  return candidate;
}

function draftFromPreset(preset: ProviderPreset | null, existingIds: string[]): CustomProviderDefinition {
  if (!preset) return emptyProvider();
  const models = preset.defaultModels.map(([id, name]) => ({ id, name, capabilities: [] }));
  return {
    ...emptyProvider(),
    providerId: uniqueProviderId(preset.id === "llm" ? "text-ai" : preset.id, existingIds),
    displayName: preset.id === "llm" ? "" : preset.label,
    baseUrl: preset.defaultBaseUrl,
    purpose: derivedPurpose(preset),
    presetId: preset.id,
    runtime: JSON.parse(JSON.stringify(preset.runtime)) as typeof preset.runtime,
    models: models.length ? models : [{ id: "", name: "" }],
  };
}

/** Guided "Custom REST API" fields compiled into an image.generate operation. */
type CustomRestFields = {
  method: string;
  path: string;
  bodyTemplate: string;
  outputKind: "image-url" | "image-base64" | "video-url" | "binary-image";
  responsePath: string;
};

const defaultCustomRest = (): CustomRestFields => ({
  method: "POST",
  path: "/generate",
  bodyTemplate: '{\n  "model": "{{model}}",\n  "prompt": "{{prompt}}"\n}',
  outputKind: "image-url",
  responsePath: "result.images.0.url",
});

function customRestFromDraft(draft: CustomProviderDefinition): CustomRestFields {
  const endpoint = draft.runtime?.operations?.["image.generate"];
  if (!endpoint) return defaultCustomRest();
  const response = endpoint.response;
  const outputKind: CustomRestFields["outputKind"] = response.binaryResponse
    ? "binary-image"
    : response.base64Path
      ? "image-base64"
      : (response.mimeType ?? "").startsWith("video/")
        ? "video-url"
        : "image-url";
  const mapping = (endpoint.requestMapping ?? {}) as Record<string, unknown>;
  return {
    method: endpoint.method ?? "POST",
    path: endpoint.pathTemplate ?? "/generate",
    bodyTemplate: JSON.stringify(mapping, null, 2),
    outputKind,
    responsePath:
      response.urlPath ?? response.base64Path ?? response.outputsPath ?? "result.images.0.url",
  };
}

function applyCustomRest(draft: CustomProviderDefinition, fields: CustomRestFields): CustomProviderDefinition {
  const response =
    fields.outputKind === "binary-image"
      ? {
          binaryResponse: true,
          mimeType: "image/jpeg",
          filename: "generated.jpg",
          outputsPath: null,
          urlPath: null,
          base64Path: null,
          providerRequestIdPath: null,
        }
      : fields.outputKind === "image-base64"
        ? {
            binaryResponse: false,
            mimeType: "image/png",
            filename: "generated.png",
            outputsPath: null,
            urlPath: null,
            base64Path: fields.responsePath,
            providerRequestIdPath: null,
          }
        : fields.outputKind === "video-url"
          ? {
              binaryResponse: false,
              mimeType: "video/mp4",
              filename: "generated.mp4",
              outputsPath: null,
              urlPath: fields.responsePath,
              base64Path: null,
              providerRequestIdPath: null,
            }
          : {
              binaryResponse: false,
              mimeType: "image/png",
              filename: "generated.png",
              outputsPath: null,
              urlPath: fields.responsePath,
              base64Path: null,
              providerRequestIdPath: null,
            };
  let mapping: unknown = {};
  try {
    mapping = JSON.parse(fields.bodyTemplate);
  } catch {
    mapping = draft.runtime?.operations?.["image.generate"]?.requestMapping ?? {};
  }
  const operations = { ...(draft.runtime?.operations ?? {}) };
  operations["image.generate"] = {
    ...(operations["image.generate"] ?? {}),
    method: fields.method,
    pathTemplate: fields.path,
    requestType: "json",
    requestMapping: mapping,
    response,
  };
  const runtime: NonNullable<CustomProviderDefinition["runtime"]> = {
    accountId: draft.runtime?.accountId ?? null,
    headers: draft.runtime?.headers ?? {},
    errorMapping: draft.runtime?.errorMapping ?? null,
    auth: draft.runtime?.auth ?? { mode: "bearer", credentialName: null },
    operations,
  };
  return { ...draft, runtime };
}

export function ProviderSettings({ projectRootPath }: { projectRootPath: string }) {
  const [providers, setProviders] = useState<CustomProviderDefinition[]>([]);
  const [presets, setPresets] = useState<ProviderPreset[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [draft, setDraft] = useState<CustomProviderDefinition>(emptyProvider);
  const [advanced, setAdvanced] = useState(false);
  const [customRest, setCustomRest] = useState<CustomRestFields>(defaultCustomRest);
  const [operationsText, setOperationsText] = useState("");
  const [operationsError, setOperationsError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const projectRef = useRef(projectRootPath);
  const selectionRef = useRef(selectedId);
  const operationRef = useRef(0);
  projectRef.current = projectRootPath;
  selectionRef.current = selectedId;

  const activePreset = presets.find((preset) => preset.id === draft.presetId) ?? null;
  const isCustomRest = draft.presetId === "custom";
  const requiresCredential = activePreset
    ? activePreset.auth.mode !== "none"
    : draft.runtime?.auth?.mode !== "none";

  async function refresh(expectedProject = projectRootPath) {
    const next = (await listCustomProviders(expectedProject)) ?? [];
    if (projectRef.current !== expectedProject) return [];
    setProviders(next);
    return next;
  }
  useEffect(() => {
    const expectedProject = projectRootPath;
    operationRef.current += 1;
    setProviders([]); setSelectedId(""); setDraft(emptyProvider()); setError(null); setMessage(null); setTesting(false); setAdvanced(false);
    refresh(expectedProject).catch((reason) => { if (projectRef.current === expectedProject) setError(describeError(reason)); });
    Promise.resolve(listProviderPresets())
      .then((next) => {
        if (projectRef.current !== expectedProject) return;
        const public_presets = (next ?? []).filter((preset) => !preset.internal);
        setPresets([LLM_PRESET, ...public_presets]);
      })
      .catch(() => {
        if (projectRef.current === expectedProject) setPresets([LLM_PRESET]);
      });
  }, [projectRootPath]);

  // Default the empty form to the first preset once the catalog loads.
  useEffect(() => {
    if (!presets.length) return;
    if (draft.presetId || draft.providerId || selectedId) return;
    const defaultPreset = presets.find((preset) => preset.id !== "llm") ?? presets[0];
    const fresh = draftFromPreset(defaultPreset, providers.map((provider) => provider.providerId));
    setDraft(fresh);
    setCustomRest(customRestFromDraft(fresh));
    setOperationsText(JSON.stringify(fresh.runtime?.operations ?? {}, null, 2));
    // Only react to the catalog arriving, not to every draft edit.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [presets]);

  function resetOperationsEditor(next: CustomProviderDefinition) {
    setOperationsText(JSON.stringify(next.runtime?.operations ?? {}, null, 2));
    setOperationsError(null);
  }

  function selectProvider(id: string) {
    operationRef.current += 1;
    selectionRef.current = id;
    setSelectedId(id);
    const next = providers.find((provider) => provider.providerId === id) ?? emptyProvider();
    setDraft(next);
    setCustomRest(customRestFromDraft(next));
    resetOperationsEditor(next);
    // Legacy rows carry no preset; show their declarative configuration.
    setAdvanced(!next.presetId && next.purpose !== "llm");
    setError(null); setMessage(null); setTesting(false);
  }

  function choosePreset(presetId: string) {
    operationRef.current += 1;
    setTesting(false);
    setError(null); setMessage(null);
    const preset = presets.find((item) => item.id === presetId) ?? null;
    const next = draftFromPreset(preset, providers.map((provider) => provider.providerId));
    setDraft(next);
    setCustomRest(customRestFromDraft(next));
    resetOperationsEditor(next);
  }

  function updateDraft<K extends keyof CustomProviderDefinition>(key: K, value: CustomProviderDefinition[K]) {
    operationRef.current += 1;
    setTesting(false);
    setDraft((current) => ({ ...current, [key]: value }));
  }

  function updateRuntime(patch: Partial<NonNullable<CustomProviderDefinition["runtime"]>>) {
    operationRef.current += 1;
    setTesting(false);
    setDraft((current) => ({
      ...current,
      runtime: {
        accountId: current.runtime?.accountId ?? null,
        headers: current.runtime?.headers ?? {},
        errorMapping: current.runtime?.errorMapping ?? null,
        auth: current.runtime?.auth ?? { mode: "bearer", credentialName: null },
        operations: current.runtime?.operations ?? {},
        ...patch,
      },
    }));
  }

  function applyOperationsText() {
    try {
      const parsed = JSON.parse(operationsText) as NonNullable<
        CustomProviderDefinition["runtime"]
      >["operations"];
      setOperationsError(null);
      updateRuntime({ operations: parsed });
    } catch (reason) {
      setOperationsError(`Operations JSON is not valid: ${describeError(reason)}`);
    }
  }

  async function save() {
    const operation = ++operationRef.current;
    const expectedProject = projectRootPath;
    setTesting(false); setError(null); setMessage(null);
    const preset = activePreset;
    let working = draft;
    if (isCustomRest) working = applyCustomRest(working, customRest);
    let providerId = working.providerId.trim();
    if (!providerId && preset) providerId = uniqueProviderId(preset.id, providers.map((provider) => provider.providerId));
    const purpose: CustomProviderDefinition["purpose"] = preset ? derivedPurpose(preset) : working.purpose;
    const normalized: CustomProviderDefinition = {
      ...working,
      providerId,
      purpose,
      displayName: working.displayName.trim(),
      baseUrl: working.baseUrl.trim(),
      models: working.models.map((model) => ({ id: model.id.trim(), name: model.name.trim(), capabilities: model.capabilities ?? [] })),
      headers: working.headers.map((header) => ({ name: header.name.trim(), ...(header.value ? { value: header.value } : {}) })),
      runtime: {
        accountId: working.runtime?.accountId ?? null,
        headers: working.runtime?.headers ?? {},
        errorMapping: working.runtime?.errorMapping ?? null,
        auth: working.runtime?.auth ?? { mode: "bearer", credentialName: null },
        operations: working.runtime?.operations ?? {},
      },
      ...(working.apiKey ? { apiKey: working.apiKey } : {}),
    };
    if (!normalized.displayName || !/^https?:\/\/\S+$/.test(normalized.baseUrl)) { setError("Display name and a valid HTTP(S) base URL are required."); return; }
    if (!normalized.models.length || normalized.models.some((model) => !model.id || !model.name)) { setError("Add at least one complete model."); return; }
    if (isCustomRest && !customRest.responsePath.trim()) { setError("Tell Cinery where to find the result in the response (Response path)."); return; }
    try {
      const saved = await upsertCustomProvider(expectedProject, normalized);
      if (projectRef.current !== expectedProject || operationRef.current !== operation) return;
      await refresh(expectedProject);
      if (projectRef.current !== expectedProject || operationRef.current !== operation) return;
      selectionRef.current = saved.providerId;
      setSelectedId(saved.providerId);
      setDraft(saved);
      setCustomRest(customRestFromDraft(saved));
      resetOperationsEditor(saved);
      setMessage("AI service saved.");
    } catch (reason) {
      if (projectRef.current === expectedProject && operationRef.current === operation) setError(describeError(reason));
    }
  }
  async function remove() {
    if (!draft.providerId) return;
    const operation = ++operationRef.current;
    const expectedProject = projectRootPath;
    const providerId = draft.providerId;
    setTesting(false);
    try {
      await deleteCustomProvider(expectedProject, providerId);
      if (projectRef.current !== expectedProject || operationRef.current !== operation) return;
      const next = providers.filter((provider) => provider.providerId !== providerId);
      setProviders(next);
      selectionRef.current = "";
      setSelectedId("");
      setDraft(emptyProvider());
      setCustomRest(defaultCustomRest());
      setMessage("AI service removed.");
    } catch (reason) {
      if (projectRef.current === expectedProject && operationRef.current === operation) setError(describeError(reason));
    }
  }
  async function testConnection() {
    if (!selectedId) return;
    const operation = ++operationRef.current;
    const expectedProject = projectRootPath;
    const providerId = selectedId;
    setTesting(true); setError(null); setMessage(null);
    try {
      const result = await testCustomProviderConnection(expectedProject, providerId);
      if (projectRef.current !== expectedProject || selectionRef.current !== providerId || operationRef.current !== operation) return;
      if (result.connected) setMessage(result.message); else setError(result.message);
    } catch (reason) {
      if (projectRef.current === expectedProject && selectionRef.current === providerId && operationRef.current === operation) setError(describeError(reason));
    } finally {
      if (projectRef.current === expectedProject && selectionRef.current === providerId && operationRef.current === operation) setTesting(false);
    }
  }

  const showApiKey = draft.purpose !== "llm" ? requiresCredential || Boolean(draft.apiKeyHint) : true;
  const apiKeyLabel = draft.presetId === "cloudflare-workers-ai" ? "API token" : "API key (optional)";

  return <section className="provider-settings" aria-labelledby="provider-settings-title">
    <header className="workflow-panel-header"><div><h2 id="provider-settings-title">Connect an AI service</h2><p>Cinery generates images and video through an AI service you connect. Pick a service below and enter your credentials — everything else is pre-configured, and you can test the connection before using it. Keys are stored securely in your operating system's credential vault, never in the project folder.</p></div></header>
    {error ? <p role="alert">{error}</p> : null}{message ? <p role="status">{message}</p> : null}
    <div className="workflow-form-actions"><button type="button" onClick={() => { operationRef.current += 1; selectionRef.current = ""; setSelectedId(""); const next = draftFromPreset(presets[0] ?? null, providers.map((provider) => provider.providerId)); setDraft(next); setCustomRest(customRestFromDraft(next)); resetOperationsEditor(next); setAdvanced(false); setError(null); setMessage(null); setTesting(false); }}>Add a service</button></div>
    {providers.length ? <label>Saved providers<select aria-label="Saved providers" value={selectedId} onChange={(event) => selectProvider(event.target.value)}><option value="">Select a provider</option>{providers.map((provider) => <option key={provider.providerId} value={provider.providerId}>{provider.displayName} ({provider.purpose})</option>)}</select></label> : <p>No AI service connected yet. Pick a connection type below to add one.</p>}

    <fieldset><legend>What do you want to connect?</legend>
      {presets.length ? presets.map((preset) => <label key={preset.id}><input type="radio" name="provider-preset" value={preset.id} checked={draft.presetId === preset.id} onChange={() => choosePreset(preset.id)} /> {preset.label}<br /><span className="workflow-field-help">{preset.description}</span></label>)
        : <p className="workflow-field-help">Loading connection types…</p>}
    </fieldset>

    <label>Display name<input value={draft.displayName} onChange={(event) => updateDraft("displayName", event.target.value)} placeholder="My AI Service" /></label>

    {activePreset?.requiresAccountId ? <label>Account ID<input value={draft.runtime?.accountId ?? ""} onChange={(event) => updateRuntime({ accountId: event.target.value })} placeholder="Cloudflare account ID" aria-describedby="account-id-help" /><span id="account-id-help" className="workflow-field-help">Find it in the Cloudflare dashboard overview. It is not a secret.</span></label> : null}

    {isCustomRest ? <fieldset><legend>Custom REST API</legend>
      <label>HTTP method<select value={customRest.method} onChange={(event) => { const next = { ...customRest, method: event.target.value }; setCustomRest(next); setDraft((current) => applyCustomRest(current, next)); }}><option value="POST">POST</option><option value="GET">GET</option><option value="PUT">PUT</option></select></label>
      <label>Request path<input value={customRest.path} onChange={(event) => { const next = { ...customRest, path: event.target.value }; setCustomRest(next); setDraft((current) => applyCustomRest(current, next)); }} placeholder="/generate" /></label>
      <label>Request body (JSON template)<textarea rows={5} value={customRest.bodyTemplate} onChange={(event) => { const next = { ...customRest, bodyTemplate: event.target.value }; setCustomRest(next); }} onBlur={(event) => { try { JSON.parse(event.target.value); setDraft((current) => applyCustomRest(current, customRest)); } catch { setError("The request body template must be valid JSON. Use {{prompt}}, {{model}}, {{seed}}, {{width}}, {{height}} as placeholders."); } }} /></label>
      <span className="workflow-field-help">Placeholders: {"{{prompt}} {{model}} {{seed}} {{width}} {{height}} {{steps}}"} — unset values are left out of the request.</span>
      <label>Output type<select value={customRest.outputKind} onChange={(event) => { const next = { ...customRest, outputKind: event.target.value as CustomRestFields["outputKind"] }; setCustomRest(next); setDraft((current) => applyCustomRest(current, next)); }}><option value="image-url">Image URL in the response</option><option value="image-base64">Image as base64 text in the response</option><option value="video-url">Video URL in the response</option><option value="binary-image">The response body is the image itself</option></select></label>
      {customRest.outputKind !== "binary-image" ? <label>Response path<input value={customRest.responsePath} onChange={(event) => { const next = { ...customRest, responsePath: event.target.value }; setCustomRest(next); setDraft((current) => applyCustomRest(current, next)); }} placeholder="result.images.0.url" aria-describedby="response-path-help" /><span id="response-path-help" className="workflow-field-help">Dot path to the generated file, e.g. data.0.url or result.image.</span></label> : null}
    </fieldset> : null}

    <label>Base URL<input type="url" value={draft.baseUrl} onChange={(event) => updateDraft("baseUrl", event.target.value)} placeholder="https://api.example.com/v1" aria-describedby="base-url-help" /><span id="base-url-help" className="workflow-field-help">The service's API address, from its documentation. Most OpenAI-compatible services end in /v1.</span></label>

    {showApiKey ? <label>{apiKeyLabel}<input type="password" autoComplete="off" value={draft.apiKey ?? ""} onChange={(event) => updateDraft("apiKey", event.target.value)} placeholder={draft.apiKeyHint ? `Stored in vault: ${draft.apiKeyHint}` : activePreset?.auth.mode === "none" ? "Not required for this service" : ""} aria-describedby={draft.apiKeyHint ? "api-key-hint" : undefined} /></label> : null}
    {draft.apiKeyHint ? <p id="api-key-hint" className="api-key-hint">Stored credential: <code>{draft.apiKeyHint}</code> — leave the field empty to keep it.</p> : null}

    <fieldset><legend>Models</legend>{draft.models.map((model, index) => <div key={`model-${index}`}><label>Model ID<input value={model.id} onChange={(event) => updateDraft("models", draft.models.map((item, itemIndex) => itemIndex === index ? { ...item, id: event.target.value } : item))} /></label><label>Model name<input value={model.name} onChange={(event) => updateDraft("models", draft.models.map((item, itemIndex) => itemIndex === index ? { ...item, name: event.target.value } : item))} /></label><button type="button" onClick={() => updateDraft("models", draft.models.filter((_, itemIndex) => itemIndex !== index))} disabled={draft.models.length === 1}>Remove model</button></div>)}<button type="button" onClick={() => updateDraft("models", [...draft.models, { id: "", name: "" }])}>Add model</button></fieldset>

    <div className="workflow-form-actions"><button type="button" aria-expanded={advanced} onClick={() => setAdvanced((current) => !current)}>{advanced ? "Hide advanced settings" : "Advanced settings"}</button></div>

    {advanced ? <fieldset><legend>Advanced settings</legend>
      <label>Provider ID<input value={draft.providerId} onChange={(event) => updateDraft("providerId", event.target.value)} placeholder="my-image-provider" aria-describedby="provider-id-help" /><span id="provider-id-help" className="workflow-field-help">A short internal name, lowercase letters, numbers, hyphens. It never appears in your generated work.</span></label>
      <label>Authentication mode<select value={draft.runtime?.auth?.mode ?? "bearer"} onChange={(event) => updateRuntime({ auth: { mode: event.target.value as ProviderAuthMode, credentialName: draft.runtime?.auth?.credentialName ?? null } })}><option value="bearer">Bearer token (Authorization header)</option><option value="header">API key in a custom header</option><option value="query">API key in a query parameter</option><option value="none">No authentication</option></select></label>
      {draft.runtime?.auth?.mode === "header" || draft.runtime?.auth?.mode === "query" ? <label>Credential name<input value={draft.runtime?.auth?.credentialName ?? ""} onChange={(event) => updateRuntime({ auth: { mode: draft.runtime?.auth?.mode ?? "header", credentialName: event.target.value } })} placeholder={draft.runtime?.auth?.mode === "header" ? "x-api-key" : "key"} /></label> : null}
      <fieldset><legend>Headers (optional)</legend>{draft.headers.map((header, index) => <div key={`header-${index}`}><label>Header<input value={header.name} onChange={(event) => updateDraft("headers", draft.headers.map((item, itemIndex) => itemIndex === index ? { ...item, name: event.target.value } : item))} /></label><label>Value<input type="password" autoComplete="off" value={header.value ?? ""} onChange={(event) => updateDraft("headers", draft.headers.map((item, itemIndex) => itemIndex === index ? { ...item, value: event.target.value } : item))} /></label><button type="button" onClick={() => updateDraft("headers", draft.headers.filter((_, itemIndex) => itemIndex !== index))}>Remove header</button></div>)}<button type="button" onClick={() => updateDraft("headers", [...draft.headers, { name: "", value: "" }])}>Add header</button></fieldset>
      {draft.purpose !== "llm" ? <label>Operations (JSON)<textarea rows={12} value={operationsText} onChange={(event) => setOperationsText(event.target.value)} onBlur={applyOperationsText} aria-describedby="operations-help" /><span id="operations-help" className="workflow-field-help">Full endpoint definitions per operation (method, path template, request mapping, response mapping, async polling). Changes apply when you leave the field.</span></label> : null}
      {operationsError ? <p role="alert">{operationsError}</p> : null}
    </fieldset> : null}

    <div className="workflow-form-actions"><button type="button" onClick={save}>Save provider</button>{selectedId ? <button type="button" className="workflow-secondary-inline" onClick={testConnection} disabled={testing}>{testing ? "Testing…" : "Test connection"}</button> : null}{selectedId ? <button type="button" className="workflow-danger" onClick={remove}>Delete provider</button> : null}</div>
  </section>;
}
