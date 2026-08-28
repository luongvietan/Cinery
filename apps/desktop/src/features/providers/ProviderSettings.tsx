import type { ProviderCapabilities, ProviderConfigurationStatus } from "@cinematic/domain";
import { useEffect, useState } from "react";
import { describeError } from "../../lib/errors";
import { configureProvider, getProviderCapabilities, getProviderConfigurationStatus, listProviderModels, listProviders, removeProviderCredentials, validateProviderConfiguration } from "../workflows/api";

export function ProviderSettings({ projectRootPath }: { projectRootPath: string }) {
  const [providers, setProviders] = useState<string[]>([]);
  const [providerId, setProviderId] = useState("mock");
  const [models, setModels] = useState<string[]>([]);
  const [modelId, setModelId] = useState("");
  const [capabilities, setCapabilities] = useState<ProviderCapabilities | null>(null);
  const [status, setStatus] = useState<ProviderConfigurationStatus | null>(null);
  const [credentialReference, setCredentialReference] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => { listProviders().then(setProviders).catch((reason) => setError(describeError(reason))); }, []);
  useEffect(() => {
    setError(null);
    Promise.all([getProviderCapabilities(providerId), listProviderModels(providerId), getProviderConfigurationStatus(projectRootPath, providerId)])
      .then(([nextCapabilities, nextModels, nextStatus]) => { setCapabilities(nextCapabilities); setModels(nextModels); setModelId(nextStatus.defaultModel ?? nextModels[0] ?? ""); setStatus(nextStatus); setCredentialReference(nextStatus.credentialReference ?? ""); })
      .catch((reason) => setError(describeError(reason)));
  }, [projectRootPath, providerId]);

  async function save() {
    setError(null); setMessage(null);
    try {
      const next = await configureProvider(projectRootPath, { providerId, enabled: true, credentialReference: credentialReference || null, defaultModel: modelId || null, endpoint: null, requestTimeoutSeconds: 60, pollingIntervalSeconds: 3 });
      setStatus(next); setMessage("Provider configuration saved.");
    } catch (reason) { setError(describeError(reason)); }
  }

  async function validate() {
    setError(null); setMessage(null);
    try { await validateProviderConfiguration(providerId); setMessage("Provider configuration is valid."); } catch (reason) { setError(describeError(reason)); }
  }

  async function removeCredential() {
    setError(null); setMessage(null);
    try { await removeProviderCredentials(projectRootPath, providerId); setCredentialReference(""); setStatus((current) => current ? { ...current, credentialConfigured: false, credentialReference: null } : current); setMessage("Credential reference removed."); } catch (reason) { setError(describeError(reason)); }
  }

  return <section className="provider-settings" aria-labelledby="provider-settings-title">
    <header className="workflow-panel-header"><div><h2 id="provider-settings-title">Provider settings</h2><p>Credentials stay in the backend; this screen stores only a credential reference.</p></div></header>
    {error ? <p role="alert">{error}</p> : null}{message ? <p role="status">{message}</p> : null}
    <label>Provider<select value={providerId} onChange={(event) => setProviderId(event.target.value)}>{providers.map((provider) => <option key={provider} value={provider}>{provider}</option>)}</select></label>
    <label>Model<select value={modelId} onChange={(event) => setModelId(event.target.value)}>{models.map((model) => <option key={model} value={model}>{model}</option>)}</select></label>
    <label>Credential environment variable<input type="password" autoComplete="off" value={credentialReference} onChange={(event) => setCredentialReference(event.target.value)} placeholder="e.g. OPENAI_API_KEY" /></label>
    <p>{status?.credentialConfigured ? "Credential configured" : "Credential not configured"}</p>
    {capabilities ? <p>Supports: {capabilities.mediaTypes.join(", ")}{capabilities.supportsCancel ? " · cancellation" : ""}{capabilities.supportsProgress ? " · progress" : ""}</p> : null}
    <div className="workflow-form-actions"><button type="button" onClick={save}>Save configuration</button><button type="button" className="workflow-secondary-inline" onClick={validate}>Validate</button><button type="button" className="workflow-danger" onClick={removeCredential}>Remove credential</button></div>
  </section>;
}
