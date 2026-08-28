import { useEffect, useState } from "react";
import type { ProviderCapabilities, ProviderConfigurationStatus } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { configureProvider, getProviderCapabilities, getProviderConfigurationStatus, listProviderModels, listProviders, removeProviderCredentials, saveProviderCredential, validateProviderConfiguration } from "../workflows/api";

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

  useEffect(() => { listProviders().then(setProviders).catch((reason) => setError(describeError(reason))); }, []);
  useEffect(() => {
    setError(null);
    setSecret("");
    Promise.all([getProviderCapabilities(providerId), listProviderModels(providerId), getProviderConfigurationStatus(projectRootPath, providerId)])
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
    <label>Provider<select value={providerId} onChange={(event) => setProviderId(event.target.value)}>{providers.map((provider) => <option key={provider} value={provider}>{provider}</option>)}</select></label>
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
  </section>;
}
