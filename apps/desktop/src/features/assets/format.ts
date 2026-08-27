import { ASSET_TYPES, type AssetType } from "@cinematic/domain";

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
