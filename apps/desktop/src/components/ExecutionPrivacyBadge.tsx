import type { ReactNode } from "react";

interface ExecutionPrivacyBadgeProps {
  /// The execution location: "local" or "cloud:provider-name"
  location: "local" | `cloud:${string}`;
  /// The provider/adapter ID
  providerId?: string;
  /// The model ID
  modelId?: string;
  /// Optional className for styling
  className?: string;
}

/// Displays a privacy disclosure badge showing whether execution is LOCAL or CLOUD-based
/// Must be shown before generation/execution starts
export function ExecutionPrivacyBadge({
  location,
  providerId,
  modelId,
  className = "",
}: ExecutionPrivacyBadgeProps): ReactNode {
  const isLocal = location === "local";
  const badge = isLocal ? "LOCAL" : formatCloudBadge(location);

  return (
    <div className={`execution-privacy-badge ${className}`}>
      <span className={`badge ${isLocal ? "local" : "cloud"}`}>{badge}</span>
      {providerId && <span className="provider-info">{providerId}</span>}
      {modelId && <span className="model-info">{modelId}</span>}
    </div>
  );
}

function formatCloudBadge(location: string): string {
  if (location.startsWith("cloud:")) {
    const provider = location.slice(6);
    return `CLOUD: ${provider}`;
  }
  return "CLOUD";
}

interface PrivacyBadgeDisplayProps {
  location?: string;
  providerId?: string;
  modelId?: string;
  before?: ReactNode;
  after?: ReactNode;
}

/// Utility component that handles location strings and shows disclosure badge
export function PrivacyBadgeDisplay({
  location,
  providerId,
  modelId,
  before,
  after,
}: PrivacyBadgeDisplayProps): ReactNode {
  if (!location) return null;

  const normalizedLocation: "local" | `cloud:${string}` =
    location === "local" ? "local" : location.startsWith("cloud:") ? (location as `cloud:${string}`) : `cloud:${location}`;

  return (
    <div className="privacy-badge-display">
      {before}
      <ExecutionPrivacyBadge location={normalizedLocation} providerId={providerId} modelId={modelId} />
      {after}
    </div>
  );
}
