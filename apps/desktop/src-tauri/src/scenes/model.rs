use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

macro_rules! string_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $value),+ }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = String;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $($value => Ok(Self::$variant),)+
                    other => Err(format!("invalid {}: {other}", stringify!($name))),
                }
            }
        }
    };
}

// ---------------------------------------------------------------------------
// Scene
// ---------------------------------------------------------------------------

/// A production Scene — an assembly of exact immutable asset-version references.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub id: String,
    pub project_id: String,
    pub ordinal: i64,
    pub title: String,
    pub summary: String,
    pub world_id: Option<String>,
    pub world_asset_version_id: Option<String>,
    pub keyframe_asset_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ---------------------------------------------------------------------------
// Scene character assignment
// ---------------------------------------------------------------------------

/// Exact Character Look (and optional Sheet) pinned to a Scene.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneCharacterAssignment {
    pub id: String,
    pub scene_id: String,
    pub character_entity_id: String,
    pub look_asset_version_id: String,
    pub sheet_asset_version_id: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ---------------------------------------------------------------------------
// Scene prop assignment
// ---------------------------------------------------------------------------

/// Exact Prop Plate version pinned to a Scene.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenePropAssignment {
    pub id: String,
    pub scene_id: String,
    pub prop_asset_version_id: String,
    pub label: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// Scene TBD binding
// ---------------------------------------------------------------------------

string_enum!(TbdDecisionKind {
    PreserveUnknown => "preserve_unknown",
    NotApplicable => "not_applicable",
});

/// Persisted explicit handling decision for a protected TBD on a Scene.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneTbdBinding {
    pub id: String,
    pub scene_id: String,
    pub canon_tbd_id: String,
    pub topic_snapshot: String,
    pub note_snapshot: Option<String>,
    pub decision: TbdDecisionKind,
    pub justification: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ---------------------------------------------------------------------------
// Scene reference events
// ---------------------------------------------------------------------------

string_enum!(SceneReferenceKind {
    World => "world",
    CharacterLook => "character_look",
    CharacterSheet => "character_sheet",
    Prop => "prop",
});

string_enum!(SceneReferenceAction {
    Pin => "pin",
    Upgrade => "upgrade",
    Remove => "remove",
});

/// Append-only audit event for Scene reference changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneReferenceEvent {
    pub id: String,
    pub scene_id: String,
    pub reference_kind: SceneReferenceKind,
    pub assignment_id: Option<String>,
    pub action: SceneReferenceAction,
    pub from_version_id: Option<String>,
    pub to_version_id: Option<String>,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// Scene reference health
// ---------------------------------------------------------------------------

string_enum!(SceneReferenceHealth {
    Current => "current",
    UpgradeAvailable => "upgrade_available",
    Historical => "historical",
    Broken => "broken",
});

// ---------------------------------------------------------------------------
// Scene readiness
// ---------------------------------------------------------------------------

string_enum!(SceneReadinessBlockerKind {
    TitleMissing => "title_missing",
    SummaryMissing => "summary_missing",
    WorldReferenceMissing => "world_reference_missing",
    WorldReferenceBroken => "world_reference_broken",
    CharacterReferenceBroken => "character_reference_broken",
    PropReferenceBroken => "prop_reference_broken",
    TbdDecisionRequired => "tbd_decision_required",
});

string_enum!(SceneReadinessWarningKind {
    UpgradeAvailable => "upgrade_available",
    HistoricalReference => "historical_reference",
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneReadinessBlocker {
    pub kind: SceneReadinessBlockerKind,
    pub message: String,
    /// Optional context: e.g. assignment id, TBD id, version id that triggered the blocker.
    pub context: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneReadinessWarning {
    pub kind: SceneReadinessWarningKind,
    pub message: String,
    pub context: Option<String>,
}

/// Derived readiness for keyframe generation — never persisted as a boolean.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneReadiness {
    pub ready_for_keyframe: bool,
    pub blockers: Vec<SceneReadinessBlocker>,
    pub warnings: Vec<SceneReadinessWarning>,
}
