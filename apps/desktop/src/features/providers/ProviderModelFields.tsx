import { useEffect, useState } from "react";
import type { CustomProviderDefinition, ProviderCapabilities } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
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
  onChange(value: ProviderModelSelection): void;
}

const ALWAYS_CONFIGURED = new Set(["mock", "dry_run"]);

interface ProviderOption {
  id: string;
  capabilities: ProviderCapabilities | null;
  models: string[];
  configured: boolean;
  purpose: CustomProviderDefinition["purpose"] | null;
}

export function ProviderModelFields({ projectRootPath, value, mediaType, requiresReferences, onChange }: ProviderModelFieldsProps) {
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
        (providerIds ?? []).map(async (id): Promise<ProviderOption> => {
          const custom = (customProviders ?? []).find((provider) => provider.providerId === id);
          try {
            const [capabilities, models, status] = await Promise.all([
              getProviderCapabilities(id).catch(() => null),
              listProviderModels(id, projectRootPath).catch(() => [] as string[]),
              getProviderConfigurationStatus(projectRootPath, id).catch(() => null),
            ]);
            return { id, capabilities, models: models ?? [], configured: status?.credentialConfigured ?? ALWAYS_CONFIGURED.has(id), purpose: custom?.purpose ?? null };
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

  const selected = options.find((option) => option.id === value.providerId) ?? null;
  const compatible = (option: ProviderOption): boolean => {
    if (option.purpose && option.purpose !== "legacy" && option.purpose !== mediaType) return false;
    if (!option.capabilities) return true;
    const wanted = mediaType === "image" ? "image" : "video";
    if (!option.capabilities.mediaTypes.includes(wanted)) return false;
    if (requiresReferences && !option.capabilities.supportsReferenceImage) return false;
    return true;
  };
  const incompatibleReason = (option: ProviderOption): string | null => {
    if (compatible(option)) return null;
    if (option.capabilities && !option.capabilities.mediaTypes.includes(mediaType)) return "does not support this media type";
    if (requiresReferences) return "cannot accept reference images";
    return "not compatible with this operation";
  };
  const statusText = pending
    ? "Checking provider credentials…"
    : error
      ? "Provider status unavailable"
      : selected
        ? selected.configured
          ? "Credential configured"
          : "Credential not configured"
        : "";

  function handleProviderChange(providerId: string) {
    const option = options.find((candidate) => candidate.id === providerId);
    const modelId = option?.models[0] ?? "";
    onChange({ providerId, modelId });
  }

  return <div className="provider-model-fields">
    <label htmlFor="provider-model-provider">Provider</label>
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
    {error ? <p role="alert">{error}</p> : null}
  </div>;
}
