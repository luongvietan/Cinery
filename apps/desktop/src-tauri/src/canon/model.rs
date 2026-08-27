use serde::{Deserialize, Serialize};

/// Canon entity type identifiers stored in `canon_entities.type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonEntityType {
    Story,
    Character,
    Location,
    Faction,
    WorldRule,
    ProductionRules,
}

impl CanonEntityType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Story => "story",
            Self::Character => "character",
            Self::Location => "location",
            Self::Faction => "faction",
            Self::WorldRule => "world_rule",
            Self::ProductionRules => "production_rules",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "story" => Some(Self::Story),
            "character" => Some(Self::Character),
            "location" => Some(Self::Location),
            "faction" => Some(Self::Faction),
            "world_rule" => Some(Self::WorldRule),
            "production_rules" => Some(Self::ProductionRules),
            _ => None,
        }
    }
}

/// Row shape stored in a project's `canon_entities` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonEntityRecord {
    pub id: String,
    pub project_id: String,
    #[serde(rename = "type")]
    pub entity_type: String,
    pub name: String,
    pub slug: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Row shape stored in a project's `canon_sections` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonSectionRecord {
    pub id: String,
    pub entity_id: String,
    pub key: String,
    pub value: serde_json::Value,
    pub status: String,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
    pub locked_at: Option<String>,
}

/// Row shape stored in a project's `canon_section_revisions` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonSectionRevisionRecord {
    pub id: String,
    pub section_id: String,
    pub revision: i64,
    pub value: serde_json::Value,
    pub status: String,
    pub change_kind: String,
    pub reason: Option<String>,
    pub created_at: String,
}

/// Row shape stored in a project's `canon_tbds` table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonTbdRecord {
    pub id: String,
    pub project_id: String,
    pub canon_entity_id: Option<String>,
    pub section_key: Option<String>,
    pub topic: String,
    pub note: Option<String>,
    pub protected: bool,
    pub status: String,
    pub resolution_text: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub resolved_at: Option<String>,
}
