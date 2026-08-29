import { useEffect, useState } from "react";
import type { CustomProviderDefinition, ProviderCapabilities, ProviderConfigurationStatus } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { configureProvider, deleteCustomProvider, getProviderCapabilities, getProviderConfigurationStatus, listCustomProviders, listProviderModels, listProviders, removeProviderCredentials, saveProviderCredential, upsertCustomProvider, validateProviderConfiguration } from "../workflows/api";

const ALWAYS_CONFIGURED = new Set(["mock", "dry_run"]);

export function ProviderSettings({ projectRootPath }: { projectRootPath: string }) {
  const [providers, setProviders] = useState<string[]>([]);
  const [providerId, setProviderId] = useState("mock");
  const [models, setModels] = useState<string[]>([]);
  const [modelId, setModelId] = useState("");
  const [capabilities, setCapabilities] = useState<ProviderCapabilities | null>(null);
  const [status, setStatus] = useState<ProviderConfigurationStatus | null>(null);
  const [secret, setSecret] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [customProviders, setCustomProviders] = useState<CustomProviderDefinition[]>([]);
  const [custom, setCustom] = useState<CustomProviderDefinition>({ providerId: "", displayName: "", baseUrl: "", models: [{ id: "", name: "" }], headers: [] });

  useEffect(() => {
    Promise.all([listProviders(projectRootPath), Promise.resolve(listCustomProviders(projectRootPath)).catch(() => [])])
      .then(([ids, definitions]) => { const safeDefinitions = definitions ?? []; setProviders(ids ?? []); setCustomProviders(safeDefinitions); if (safeDefinitions[0]) setCustom(safeDefinitions[0]); })
      .catch((reason) => setError(describeError(reason)));
  }, [projectRootPath]);
  useEffect(() => {
    setError(null);
    setSecret("");
    Promise.all([getProviderCapabilities(providerId).catch(() => null), listProviderModels(providerId, projectRootPath), getProviderConfigurationStatus(projectRootPath, providerId)])
      .then(([nextCapabilities, nextModels, nextStatus]) => { setCapabilities(nextCapabilities); setModels(nextModels); setModelId(nextStatus.defaultModel ?? nextModels[0] ?? ""); setStatus(nextStatus); })
      .catch((reason) => setError(describeError(reason)));
  }, [projectRootPath, providerId]);

  async function save() {
    setError(null); setMessage(null);
    try {
      const next = await configureProvider(projectRootPath, { providerId, enabled: true, credentialReference: null, defaultModel: modelId || null, endpoint: null, requestTimeoutSeconds: 60, pollingIntervalSeconds: 3 });
      setStatus(next); setMessage("Provider configuration saved.");
    } catch (reason) { setError(describeError(reason)); }
  }

  async function saveCustom() {
    setError(null); setMessage(null);
    const normalized: CustomProviderDefinition = {
      ...custom,
      providerId: custom.providerId.trim(), displayName: custom.displayName.trim(), baseUrl: custom.baseUrl.trim(),
      models: custom.models.map((model) => ({ id: model.id.trim(), name: model.name.trim() })),
      headers: custom.headers.map((header) => ({ name: header.name.trim(), ...(header.value ? { value: header.value } : {}) })),
      ...(custom.apiKey ? { apiKey: custom.apiKey } : {}),
    };
    if (!/^[a-z0-9_-]+$/.test(normalized.providerId)) { setError("Provider ID must use lowercase letters, numbers, hyphens, or underscores."); return; }
    if (!normalized.displayName || !/^https?:\/\/\S+$/.test(normalized.baseUrl) || normalized.models.some((model) => !model.id || !model.name)) { setError("Enter a display name, valid base URL, and at least one complete model."); return; }
    try {
      const saved = await upsertCustomProvider(projectRootPath, normalized);
      const next = await listCustomProviders(projectRootPath);
      setCustomProviders(next); setProviders((current) => Array.from(new Set([...current, saved.providerId]))); setCustom(saved); setProviderId(saved.providerId); setMessage("Custom provider saved.");
    } catch (reason) { setError(describeError(reason)); }
  }

  async function removeCustom() {
    if (!custom.providerId) return;
    try { await deleteCustomProvider(projectRootPath, custom.providerId); const next = customProviders.filter((item) => item.providerId !== custom.providerId); setCustomProviders(next); setProviders((current) => current.filter((id) => id !== custom.providerId)); setCustom({ providerId: "", displayName: "", baseUrl: "", models: [{ id: "", name: "" }], headers: [] }); setMessage("Custom provider removed."); } catch (reason) { setError(describeError(reason)); }
  }

  async function saveCredential() {
    setError(null); setMessage(null);
    if (!secret.trim()) { setError("Enter an API key before saving."); return; }
    try {
      const next = await saveProviderCredential(projectRootPath, providerId, secret, modelId || null);
      setStatus(next);
      // The secret is write-only: clear the field immediately after success.
      setSecret("");
      setMessage("Credential saved to the operating system credential vault.");
    } catch (reason) { setError(describeError(reason)); }
  }

  async function validate() {
    setError(null); setMessage(null);
    try { await validateProviderConfiguration(projectRootPath, providerId); setMessage("Provider configuration is valid."); } catch (reason) { setError(describeError(reason)); }
  }

  async function removeCredential() {
    setError(null); setMessage(null);
    try {
      await removeProviderCredentials(projectRootPath, providerId);
      setStatus((current) => current ? { ...current, credentialConfigured: false } : current);
      setMessage("Credential removed.");
    } catch (reason) { setError(describeError(reason)); }
  }

  const needsCredential = !ALWAYS_CONFIGURED.has(providerId);
  const configured = status?.credentialConfigured ?? false;

  return <section className="provider-settings" aria-labelledby="provider-settings-title">
    <header className="workflow-panel-header"><div><h2 id="provider-settings-title">Provider settings</h2><p>API keys are stored in the operating system credential vault and are never returned to the app.</p></div></header>
    {error ? <p role="alert">{error}</p> : null}{message ? <p role="status">{message}</p> : null}
    <label>Provider<select value={providerId} onChange={(event) => { const id = event.target.value; setProviderId(id); const definition = customProviders.find((item) => item.providerId === id); if (definition) setCustom(definition); }}>{providers.map((provider) => <option key={provider} value={provider}>{customProviders.find((item) => item.providerId === provider)?.displayName ?? provider}</option>)}</select></label>
    <label>Model<select value={modelId} onChange={(event) => setModelId(event.target.value)}>{models.map((model) => <option key={model} value={model}>{model}</option>)}</select></label>
    {needsCredential ? <>
      <label htmlFor="provider-secret">API key</label>
      <input id="provider-secret" type="password" autoComplete="off" value={secret} onChange={(event) => setSecret(event.target.value)} placeholder={configured ? "Enter a replacement key" : "Paste your API key"} aria-describedby="provider-secret-status" />
      <p id="provider-secret-status">{configured ? "Credential configured" : "Credential not configured"}</p>
    </> : <p>This provider runs locally and needs no credential.</p>}
    {capabilities ? <p>Supports: {capabilities.mediaTypes.join(", ")}{capabilities.supportsCancel ? " · cancellation" : ""}{capabilities.supportsProgress ? " · progress" : ""}</p> : null}
    <div className="workflow-form-actions">
      <button type="button" onClick={save}>Save configuration</button>
      {needsCredential ? <button type="button" className="workflow-secondary-inline" onClick={saveCredential} disabled={!secret.trim()}>Save credential</button> : null}
      <button type="button" className="workflow-secondary-inline" onClick={validate}>Validate</button>
      {needsCredential && configured ? <button type="button" className="workflow-danger" onClick={removeCredential}>Remove credential</button> : null}
    </div>
    <hr />
    <h3>Custom provider</h3>
    <p>Configure separate providers for LLM, image, or video APIs. API keys and header values are write-only.</p>
    <label>Provider ID<input value={custom.providerId} onChange={(event) => setCustom((current) => ({ ...current, providerId: event.target.value }))} placeholder="my-image-provider" /></label>
    <label>Display name<input value={custom.displayName} onChange={(event) => setCustom((current) => ({ ...current, displayName: event.target.value }))} placeholder="My Image Provider" /></label>
    <label>Base URL<input type="url" value={custom.baseUrl} onChange={(event) => setCustom((current) => ({ ...current, baseUrl: event.target.value }))} placeholder="https://api.example.com/v1" /></label>
    <label>API key (optional)<input type="password" autoComplete="off" value={custom.apiKey ?? ""} onChange={(event) => setCustom((current) => ({ ...current, apiKey: event.target.value }))} placeholder="Leave empty for header auth" /></label>
    <fieldset><legend>Models</legend>{custom.models.map((model, index) => <div key={`model-${index}`}><label>Model ID<input value={model.id} onChange={(event) => setCustom((current) => ({ ...current, models: current.models.map((item, itemIndex) => itemIndex === index ? { ...item, id: event.target.value } : item) }))} /></label><label>Model name<input value={model.name} onChange={(event) => setCustom((current) => ({ ...current, models: current.models.map((item, itemIndex) => itemIndex === index ? { ...item, name: event.target.value } : item) }))} /></label>{custom.models.length > 1 ? <button type="button" onClick={() => setCustom((current) => ({ ...current, models: current.models.filter((_, itemIndex) => itemIndex !== index) }))}>Remove model</button> : null}</div>)}<button type="button" onClick={() => setCustom((current) => ({ ...current, models: [...current.models, { id: "", name: "" }] }))}>Add model</button></fieldset>
    <fieldset><legend>Headers (optional)</legend>{custom.headers.map((header, index) => <div key={`header-${index}`}><label>Header<input value={header.name} onChange={(event) => setCustom((current) => ({ ...current, headers: current.headers.map((item, itemIndex) => itemIndex === index ? { ...item, name: event.target.value } : item) }))} /></label><label>Value<input type="password" autoComplete="off" value={header.value ?? ""} onChange={(event) => setCustom((current) => ({ ...current, headers: current.headers.map((item, itemIndex) => itemIndex === index ? { ...item, value: event.target.value } : item) }))} /></label><button type="button" onClick={() => setCustom((current) => ({ ...current, headers: current.headers.filter((_, itemIndex) => itemIndex !== index) }))}>Remove header</button></div>)}<button type="button" onClick={() => setCustom((current) => ({ ...current, headers: [...current.headers, { name: "", value: "" }] }))}>Add header</button></fieldset>
    <div className="workflow-form-actions"><button type="button" onClick={saveCustom}>Save custom provider</button>{custom.providerId && customProviders.some((item) => item.providerId === custom.providerId) ? <button type="button" className="workflow-danger" onClick={removeCustom}>Delete custom provider</button> : null}</div>
  </section>;
}
