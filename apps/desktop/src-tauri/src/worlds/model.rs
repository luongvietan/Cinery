use crate::assets::model::AssetRecord;
use crate::canon::model::CanonEntityRecord;
use serde::{Deserialize, Serialize};

/// Production representation of a Canon Location.
///
/// A World links one `canon_entities` row of type `location` to a stable
/// `world_plate` Asset that holds its visual environment truth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct World {
    pub id: String,
    pub project_id: String,
    pub canon_location_entity_id: String,
    pub world_plate_asset_id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// World enriched with its Canon Location display data and
/// stable World Plate Asset, returned by list/get queries
/// without copying Location narrative into World storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldDetail {
    pub world: World,
    pub location: CanonEntityRecord,
    pub world_plate_asset: AssetRecord,
}
