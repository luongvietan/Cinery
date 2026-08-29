import { useEffect, useRef, useState } from "react";
import type { CustomProviderDefinition } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { deleteCustomProvider, listCustomProviders, testCustomProviderConnection, upsertCustomProvider } from "../workflows/api";

const emptyProvider = (): CustomProviderDefinition => ({ providerId: "", displayName: "", baseUrl: "", purpose: "image", models: [{ id: "", name: "" }], headers: [] });

export function ProviderSettings({ projectRootPath }: { projectRootPath: string }) {
  const [providers, setProviders] = useState<CustomProviderDefinition[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [draft, setDraft] = useState<CustomProviderDefinition>(emptyProvider);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const projectRef = useRef(projectRootPath);
  const selectionRef = useRef(selectedId);
  const operationRef = useRef(0);
  projectRef.current = projectRootPath;
  selectionRef.current = selectedId;

  async function refresh(expectedProject = projectRootPath) {
    const next = (await listCustomProviders(expectedProject)) ?? [];
    if (projectRef.current !== expectedProject) return [];
    setProviders(next);
    return next;
  }
  useEffect(() => {
    const expectedProject = projectRootPath;
    operationRef.current += 1;
    setProviders([]); setSelectedId(""); setDraft(emptyProvider()); setError(null); setMessage(null); setTesting(false);
    refresh(expectedProject).catch((reason) => { if (projectRef.current === expectedProject) setError(describeError(reason)); });
  }, [projectRootPath]);

  function selectProvider(id: string) { operationRef.current += 1; selectionRef.current = id; setSelectedId(id); setDraft(providers.find((provider) => provider.providerId === id) ?? emptyProvider()); setError(null); setMessage(null); setTesting(false); }
  function updateDraft<K extends keyof CustomProviderDefinition>(key: K, value: CustomProviderDefinition[K]) { operationRef.current += 1; setTesting(false); setDraft((current) => ({ ...current, [key]: value })); }

  async function save() {
    const operation = ++operationRef.current;
    const expectedProject = projectRootPath;
    setTesting(false); setError(null); setMessage(null);
    const normalized: CustomProviderDefinition = { ...draft, providerId: draft.providerId.trim(), displayName: draft.displayName.trim(), baseUrl: draft.baseUrl.trim(), models: draft.models.map((model) => ({ id: model.id.trim(), name: model.name.trim() })), headers: draft.headers.map((header) => ({ name: header.name.trim(), ...(header.value ? { value: header.value } : {}) })), ...(draft.apiKey ? { apiKey: draft.apiKey } : {}) };
    if (!/^[a-z0-9_-]+$/.test(normalized.providerId)) { setError("Provider ID must use lowercase letters, numbers, hyphens, or underscores."); return; }
    if (!normalized.displayName || !/^https?:\/\/\S+$/.test(normalized.baseUrl)) { setError("Display name and a valid HTTP(S) base URL are required."); return; }
    if (!normalized.models.length || normalized.models.some((model) => !model.id || !model.name)) { setError("Add at least one complete model."); return; }
    try { const saved = await upsertCustomProvider(expectedProject, normalized); if (projectRef.current !== expectedProject || operationRef.current !== operation) return; await refresh(expectedProject); if (projectRef.current !== expectedProject || operationRef.current !== operation) return; selectionRef.current = saved.providerId; setSelectedId(saved.providerId); setDraft(saved); setMessage("Custom provider saved."); } catch (reason) { if (projectRef.current === expectedProject && operationRef.current === operation) setError(describeError(reason)); }
  }
  async function remove() { if (!draft.providerId) return; const operation = ++operationRef.current; const expectedProject = projectRootPath; const providerId = draft.providerId; setTesting(false); try { await deleteCustomProvider(expectedProject, providerId); if (projectRef.current !== expectedProject || operationRef.current !== operation) return; const next = providers.filter((provider) => provider.providerId !== providerId); setProviders(next); selectionRef.current = ""; setSelectedId(""); setDraft(emptyProvider()); setMessage("Custom provider removed."); } catch (reason) { if (projectRef.current === expectedProject && operationRef.current === operation) setError(describeError(reason)); } }
  async function testConnection() {
    if (!selectedId) return;
    const operation = ++operationRef.current;
    const expectedProject = projectRootPath;
    const providerId = selectedId;
    setTesting(true); setError(null); setMessage(null);
    try { const result = await testCustomProviderConnection(expectedProject, providerId); if (projectRef.current !== expectedProject || selectionRef.current !== providerId || operationRef.current !== operation) return; if (result.connected) setMessage(result.message); else setError(result.message); } catch (reason) { if (projectRef.current === expectedProject && selectionRef.current === providerId && operationRef.current === operation) setError(describeError(reason)); } finally { if (projectRef.current === expectedProject && selectionRef.current === providerId && operationRef.current === operation) setTesting(false); }
  }

  return <section className="provider-settings" aria-labelledby="provider-settings-title">
    <header className="workflow-panel-header"><div><h2 id="provider-settings-title">Custom providers</h2><p>Add separate credentials for LLM, image, video, or any other API. API keys and header values are write-only and stored in the operating-system credential vault.</p></div></header>
    {error ? <p role="alert">{error}</p> : null}{message ? <p role="status">{message}</p> : null}
    <div className="workflow-form-actions"><button type="button" onClick={() => { operationRef.current += 1; selectionRef.current = ""; setSelectedId(""); setDraft(emptyProvider()); setError(null); setMessage(null); setTesting(false); }}>Add provider</button></div>
    {providers.length ? <label>Saved providers<select aria-label="Saved providers" value={selectedId} onChange={(event) => selectProvider(event.target.value)}><option value="">Select a provider</option>{providers.map((provider) => <option key={provider.providerId} value={provider.providerId}>{provider.displayName} ({provider.purpose})</option>)}</select></label> : <p>No custom providers configured yet.</p>}
    <label>Purpose<select value={draft.purpose} onChange={(event) => updateDraft("purpose", event.target.value as CustomProviderDefinition["purpose"])}>{draft.purpose === "legacy" ? <option value="legacy" disabled>Choose a purpose (legacy)</option> : null}<option value="llm">LLM</option><option value="image">Image</option><option value="video">Video</option></select></label>
    <label>Provider ID<input value={draft.providerId} onChange={(event) => updateDraft("providerId", event.target.value)} placeholder="my-image-provider" /></label>
    <label>Display name<input value={draft.displayName} onChange={(event) => updateDraft("displayName", event.target.value)} placeholder="My Image Provider" /></label>
    <label>Base URL<input type="url" value={draft.baseUrl} onChange={(event) => updateDraft("baseUrl", event.target.value)} placeholder="https://api.example.com/v1" /></label>
    <label>API key (optional)<input type="password" autoComplete="off" value={draft.apiKey ?? ""} onChange={(event) => updateDraft("apiKey", event.target.value)} placeholder={draft.apiKeyHint ? `Stored in vault: ${draft.apiKeyHint}` : "Leave empty for header auth"} aria-describedby={draft.apiKeyHint ? "api-key-hint" : undefined} /></label>
    {draft.apiKeyHint ? <p id="api-key-hint" className="api-key-hint">Stored credential: <code>{draft.apiKeyHint}</code> — leave the field empty to keep it.</p> : null}
    <fieldset><legend>Models</legend>{draft.models.map((model, index) => <div key={`model-${index}`}><label>Model ID<input value={model.id} onChange={(event) => updateDraft("models", draft.models.map((item, itemIndex) => itemIndex === index ? { ...item, id: event.target.value } : item))} /></label><label>Model name<input value={model.name} onChange={(event) => updateDraft("models", draft.models.map((item, itemIndex) => itemIndex === index ? { ...item, name: event.target.value } : item))} /></label><button type="button" onClick={() => updateDraft("models", draft.models.filter((_, itemIndex) => itemIndex !== index))} disabled={draft.models.length === 1}>Remove model</button></div>)}<button type="button" onClick={() => updateDraft("models", [...draft.models, { id: "", name: "" }])}>Add model</button></fieldset>
    <fieldset><legend>Headers (optional)</legend>{draft.headers.map((header, index) => <div key={`header-${index}`}><label>Header<input value={header.name} onChange={(event) => updateDraft("headers", draft.headers.map((item, itemIndex) => itemIndex === index ? { ...item, name: event.target.value } : item))} /></label><label>Value<input type="password" autoComplete="off" value={header.value ?? ""} onChange={(event) => updateDraft("headers", draft.headers.map((item, itemIndex) => itemIndex === index ? { ...item, value: event.target.value } : item))} /></label><button type="button" onClick={() => updateDraft("headers", draft.headers.filter((_, itemIndex) => itemIndex !== index))}>Remove header</button></div>)}<button type="button" onClick={() => updateDraft("headers", [...draft.headers, { name: "", value: "" }])}>Add header</button></fieldset>
    <div className="workflow-form-actions"><button type="button" onClick={save}>Save provider</button>{selectedId ? <button type="button" className="workflow-secondary-inline" onClick={testConnection} disabled={testing}>{testing ? "Testing…" : "Test connection"}</button> : null}{selectedId ? <button type="button" className="workflow-danger" onClick={remove}>Delete provider</button> : null}</div>
  </section>;
}
