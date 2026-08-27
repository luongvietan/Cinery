use serde::{Deserialize, Serialize};

/// Row shape stored in a project's `assets` table.
///
/// Serialized as camelCase to match `packages/domain/src/asset.ts`'s
/// `Asset` interface, and doubles as the DTO returned to the frontend --
/// there is no separate wire type since the shapes are identical.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetRecord {
    pub id: String,
    pub project_id: String,
    #[serde(rename = "type")]
    pub asset_type: String,
    pub label: String,
    pub owner_entity_id: Option<String>,
    pub canonical_version_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Row shape stored in a project's `asset_versions` table.
///
/// Serialized as camelCase to match
/// `packages/domain/src/asset.ts`'s `AssetVersion` interface.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetVersionRecord {
    pub id: String,
    pub asset_id: String,
    pub version_number: i64,
    pub status: String,
    pub file_path: String,
    pub thumbnail_path: String,
    pub sha256: String,
    pub original_filename: String,
    pub mime_type: String,
    pub byte_size: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub parent_version_id: Option<String>,
    pub created_at: String,
}

/// Summary row for the asset sidebar: core asset fields plus version
/// counts and canonical preview metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetSummaryRecord {
    pub id: String,
    pub project_id: String,
    #[serde(rename = "type")]
    pub asset_type: String,
    pub label: String,
    pub owner_entity_id: Option<String>,
    pub canonical_version_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub version_count: i64,
    pub canonical_version_number: Option<i64>,
    pub preview_thumbnail_path: Option<String>,
}

/// An asset together with all of its versions, ordered newest-first.
/// Matches `packages/domain/src/asset.ts`'s `AssetWithVersions` interface.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetWithVersions {
    pub asset: AssetRecord,
    pub versions: Vec<AssetVersionRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalPromotionResult {
    pub asset: AssetRecord,
    pub promoted_version: AssetVersionRecord,
    pub superseded_version_id: Option<String>,
}
