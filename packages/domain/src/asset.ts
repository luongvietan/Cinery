export const ASSET_TYPES = [
  "face_lock",
  "outfit",
  "character_sheet",
  "world_plate",
  "shot_keyframe",
  "prop_plate",
  "image",
  "video",
  "audio",
] as const;

export type AssetType = (typeof ASSET_TYPES)[number];

export const ASSET_VERSION_STATUSES = [
  "draft",
  "generated",
  "candidate",
  "qa_failed",
  "repairing",
  "approved",
  "canonical",
  "superseded",
] as const;

export type AssetVersionStatus = (typeof ASSET_VERSION_STATUSES)[number];

export const ASSET_VERSION_ORIGINS = ["imported", "generated"] as const;

export type AssetVersionOrigin = (typeof ASSET_VERSION_ORIGINS)[number];

export interface Asset {
  id: string;
  projectId: string;
  type: AssetType;
  label: string;
  ownerEntityId: string | null;
  canonicalVersionId: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface AssetVersion {
  id: string;
  assetId: string;
  versionNumber: number;
  status: AssetVersionStatus;
  filePath: string;
  thumbnailPath: string;
  sha256: string;
  originalFilename: string;
  mimeType: "image/png" | "image/jpeg" | "image/webp";
  byteSize: number;
  width: number | null;
  height: number | null;
  parentVersionId: string | null;
  createdAt: string;
  origin?: AssetVersionOrigin;
  generationArtifactId?: string | null;
}

export interface AssetSummary {
  id: string;
  projectId: string;
  type: AssetType;
  label: string;
  ownerEntityId: string | null;
  canonicalVersionId: string | null;
  createdAt: string;
  updatedAt: string;
  versionCount: number;
  canonicalVersionNumber: number | null;
  previewThumbnailPath: string | null;
}

export interface AssetWithVersions {
  asset: Asset;
  versions: AssetVersion[];
}

export interface CreateAssetInput {
  projectRootPath: string;
  type: AssetType;
  label: string;
  ownerEntityId?: string | null;
}

export interface ImportAssetVersionInput {
  projectRootPath: string;
  assetId: string;
  sourcePath: string;
  parentVersionId?: string | null;
}

export interface PromoteAssetVersionInput {
  projectRootPath: string;
  assetVersionId: string;
}

export interface CanonicalPromotionResult {
  asset: Asset;
  promotedVersion: AssetVersion;
  supersededVersionId: string | null;
}

export function formatVersionNumber(value: number): string {
  return `v${String(value).padStart(3, "0")}`;
}

export function validateAssetLabel(value: string): string {
  const trimmed = value.trim();
  if (trimmed.length < 1 || trimmed.length > 160) {
    throw new Error("Asset label must contain 1 to 160 characters");
  }
  return trimmed;
}

export function validateSprintOneAssetType(value: AssetType): AssetType {
  if (value === "video" || value === "audio") {
    throw new Error("This asset type is not supported in Sprint 1");
  }
  return value;
}
