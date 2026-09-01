import { useEffect, useRef, useState } from "react";
import type { CustomProviderDefinition, ProviderCapabilities } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { openPanel } from "../../lib/panelNavigation";
import { getProviderCapabilities, getProviderConfigurationStatus, listCustomProviders, listProviderModels, listProviders } from "../workflows/api";

export interface ProviderModelSelection {
  providerId: string;
  modelId: string;
}

interface ProviderModelFieldsProps {
  projectRootPath: string;
  value: ProviderModelSelection;
  mediaType: "image" | "video";
  requiresReferences: boolean;
  /** When set (e.g. "video.imageToVideo"), only providers advertising the
   * capability and models listing the operation are offered. */
  requiredOperation?: "video.imageToVideo";
  onChange(value: ProviderModelSelection): void;
}

// Local-only test providers are still registered in the backend for tests and
// diagnostics, but they must never appear in a user-facing run form.
const USER_HIDDEN_PROVIDERS = new Set(["mock", "dry_run"]);

interface ProviderOption {
  id: string;
  capabilities: ProviderCapabilities | null;
  models: string[];
  configured: boolean;
  purpose: CustomProviderDefinition["purpose"] | null;
}

export function ProviderModelFields({ projectRootPath, value, mediaType, requiresReferences, requiredOperation, onChange }: ProviderModelFieldsProps) {
  const [options, setOptions] = useState<ProviderOption[]>([]);
  const [pending, setPending] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setPending(true);
    setError(null);
    Promise.resolve()
      .then(() => Promise.all([listProviders(projectRootPath), Promise.resolve(listCustomProviders(projectRootPath)).catch(() => [] as CustomProviderDefinition[])]))
      .then(([providerIds, customProviders]) => Promise.all(
        (providerIds ?? [])
          .filter((id) => !USER_HIDDEN_PROVIDERS.has(id))
          .map(async (id): Promise<ProviderOption> => {
            const custom = (customProviders ?? []).find((provider) => provider.providerId === id);
            try {
              const [capabilities, models, status] = await Promise.all([
                getProviderCapabilities(id, projectRootPath).catch(() => null),
                listProviderModels(id, projectRootPath).catch(() => [] as string[]),
                getProviderConfigurationStatus(projectRootPath, id).catch(() => null),
              ]);
              const discoveredModels = models ?? [];
              const modelOptions = custom?.models
                .filter((model) => !requiredOperation || !model.capabilities?.length || model.capabilities.includes(requiredOperation))
                .map((model) => model.id) ?? discoveredModels;
              return {
                id,
                capabilities,
                models: modelOptions,
                configured: status?.credentialConfigured ?? false,
                purpose: custom?.purpose ?? null,
              };
            } catch {
              return { id, capabilities: null, models: [], configured: false, purpose: custom?.purpose ?? null };
            }
          }),
      ))
      .then((resolved) => { if (!cancelled) setOptions(resolved); })
      .catch((reason) => { if (!cancelled) setError(describeError(reason)); })
      .finally(() => { if (!cancelled) setPending(false); });
    return () => { cancelled = true; };
  }, [projectRootPath]);

  // Auto-select the first compatible, configured service when the form opens
  // with no selection, so the user never has to understand provider ids.
  const autoSelectRef = useRef(false);
  useEffect(() => {
    if (pending || autoSelectRef.current || options.length === 0) return;
    if (value.providerId && options.some((option) => option.id === value.providerId)) {
      autoSelectRef.current = true;
      return;
    }
    const candidate = options.find((option) => {
      if (!option.configured) return false;
      if (option.purpose && option.purpose !== "legacy" && option.purpose !== mediaType) return false;
      if (option.capabilities) {
        if (!option.capabilities.mediaTypes.includes(mediaType)) return false;
        if (requiresReferences && !option.capabilities.supportsReferenceImage) return false;
        if (requiredOperation && !option.capabilities.supportsImageToVideo) return false;
      }
      return true;
    });
    autoSelectRef.current = true;
    if (candidate) {
      onChange({ providerId: candidate.id, modelId: candidate.models[0] ?? "" });
    }
  }, [pending, options, value.providerId, mediaType, requiresReferences, requiredOperation, onChange]);

  const selected = options.find((option) => option.id === value.providerId) ?? null;
  const compatible = (option: ProviderOption): boolean => {
    if (option.purpose && option.purpose !== "legacy" && option.purpose !== mediaType) return false;
    if (requiredOperation && (!option.capabilities || !option.capabilities.supportsImageToVideo)) return false;
    if (!option.capabilities) return true;
    const wanted = mediaType === "image" ? "image" : "video";
    if (!option.capabilities.mediaTypes.includes(wanted)) return false;
    if (requiresReferences && !option.capabilities.supportsReferenceImage) return false;
    return true;
  };
  const incompatibleReason = (option: ProviderOption): string | null => {
    if (compatible(option)) return null;
    if (requiredOperation && (!option.capabilities || !option.capabilities.supportsImageToVideo)) return "does not support image-to-video";
    if (option.capabilities && !option.capabilities.mediaTypes.includes(mediaType)) return "does not support this media type";
    if (requiresReferences) return "cannot accept reference images";
    return "not compatible with this operation";
  };
  const statusText = pending
    ? "Checking connection…"
    : error
      ? "AI service status unavailable"
      : selected
        ? selected.configured
          ? "Connected"
          : "This service needs its API key before it can generate anything"
        : "";

  const hasConfiguredCompatible = options.some((option) => option.configured && compatible(option));
  const needsSetup = !pending && !error && !hasConfiguredCompatible;

  function handleProviderChange(providerId: string) {
    const option = options.find((candidate) => candidate.id === providerId);
    const modelId = option?.models[0] ?? "";
    onChange({ providerId, modelId });
  }

  return <div className="provider-model-fields">
    {needsSetup ? <div className="provider-setup-hint" role="status">
      <p>Cinery generates through an AI service you connect, like an image or video API. None is connected yet.</p>
      <button type="button" onClick={() => openPanel("providers")}>Connect an AI service</button>
    </div> : null}
    <label htmlFor="provider-model-provider">AI service</label>
    <select
      id="provider-model-provider"
      value={value.providerId}
      onChange={(event) => handleProviderChange(event.target.value)}
      aria-describedby="provider-model-status"
      disabled={pending}
    >
      {options.map((option) => {
        const reason = incompatibleReason(option);
        return <option key={option.id} value={option.id} disabled={reason !== undefined && reason !== null} title={reason ?? undefined}>{option.id}{reason ? ` (${reason})` : ""}</option>;
      })}
    </select>
    <label htmlFor="provider-model-model">Model</label>
    <select
      id="provider-model-model"
      value={value.modelId}
      onChange={(event) => onChange({ providerId: value.providerId, modelId: event.target.value })}
      aria-describedby="provider-model-status"
      disabled={pending}
    >
      {(selected?.models ?? []).map((model) => <option key={model} value={model}>{model}</option>)}
    </select>
    <p id="provider-model-status" role="status">{statusText}</p>
    {selected && !selected.configured && !pending ? <button type="button" onClick={() => openPanel("providers")}>Add the API key in AI Services</button> : null}
    {error ? <p role="alert">{error}</p> : null}
  </div>;
}
