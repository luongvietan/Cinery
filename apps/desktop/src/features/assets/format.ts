import { ASSET_TYPES, type AssetType, formatVersionNumber } from "@cinematic/domain";

/**
 * The asset types Sprint 1's UI offers when creating an asset. `video` and
 * `audio` are excluded on purpose: the backend also rejects them
 * (`validateSprintOneAssetType`), but the UI shouldn't even offer them.
 */
export const SPRINT_ONE_ASSET_TYPES = ASSET_TYPES.filter(
  (type): type is Exclude<AssetType, "video" | "audio"> =>
    type !== "video" && type !== "audio",
);

/** "face_lock" -> "Face Lock", "qa_failed" -> "Qa Failed", etc. */
export function humanizeSnakeCase(value: string): string {
  return value
    .split("_")
    .filter(Boolean)
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");
}

export function humanizeAssetType(type: AssetType): string {
  return humanizeSnakeCase(type);
}

export function formatStatusLabel(status: string): string {
  return humanizeSnakeCase(status);
}

export function formatByteSize(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function formatImageDimensions(
  width: number | null,
  height: number | null,
): string {
  if (width === null || height === null) {
    return "Unknown";
  }
  return `${width} × ${height}`;
}

export function formatImageFormat(mimeType: string): string {
  const format = mimeType.split("/")[1]?.toUpperCase();
  return format ?? mimeType;
}

export function formatImportedDate(isoDate: string): string {
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  }).format(new Date(isoDate));
}

export function describeAssetListStatus(summary: {
  versionCount: number;
  canonicalVersionNumber: number | null;
}): string {
  if (summary.versionCount === 0) {
    return "No versions";
  }
  if (summary.canonicalVersionNumber !== null) {
    return `${formatVersionNumber(summary.canonicalVersionNumber)} Canonical`;
  }
  return "No canonical version";
}
