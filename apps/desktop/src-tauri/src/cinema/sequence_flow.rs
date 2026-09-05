//! Sequence-first workflow state (Joey contract).
//!
//! One persisted flow record per authoritative Scene (`world_scenes`). The
//! Scene aggregate stays the sole creative authority; this module only owns
//! the human-authored director brief and the workflow's explicit approval
//! stage. Every stage change is a guarded compare-and-set so a stale client
//! can never skip a stage or overwrite a concurrent transition.

use crate::cinema::model::{BehavioralLocks, WorldContinuity};
use crate::cinema::repository as cinema_repository;
use crate::cinema::review::read_model::resolve_canonical_video_version;
use crate::cinema::service::{ensure_canonical_version, CinemaService};
use crate::db;
use crate::error::AppError;
use crate::project::service::ProjectService;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Ordered, explicit stages of the sequence-first workflow. A sequence only
/// advances through these stages by deliberate creator action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SequenceStage {
    Draft,
    BriefLocked,
    ReferencesReady,
    PromptApproved,
    Generating,
    InReview,
    CanonicalSelected,
    ReadyForEdit,
}

impl SequenceStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            SequenceStage::Draft => "draft",
            SequenceStage::BriefLocked => "brief_locked",
            SequenceStage::ReferencesReady => "references_ready",
            SequenceStage::PromptApproved => "prompt_approved",
            SequenceStage::Generating => "generating",
            SequenceStage::InReview => "in_review",
            SequenceStage::CanonicalSelected => "canonical_selected",
            SequenceStage::ReadyForEdit => "ready_for_edit",
        }
    }

    /// The only stage that may directly follow this one. Any other target is
    /// a rejected skip or backwards move.
    pub fn successor(&self) -> Option<SequenceStage> {
        match self {
            SequenceStage::Draft => Some(SequenceStage::BriefLocked),
            SequenceStage::BriefLocked => Some(SequenceStage::ReferencesReady),
            SequenceStage::ReferencesReady => Some(SequenceStage::PromptApproved),
            SequenceStage::PromptApproved => Some(SequenceStage::Generating),
            SequenceStage::Generating => Some(SequenceStage::InReview),
            SequenceStage::InReview => Some(SequenceStage::CanonicalSelected),
            SequenceStage::CanonicalSelected => Some(SequenceStage::ReadyForEdit),
            SequenceStage::ReadyForEdit => None,
        }
    }
}

/// The emotional energy of the sequence. Creator-owned vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SequenceEnergy {
    Composed,
    Elevated,
    Kinetic,
    Violent,
}

impl SequenceEnergy {
    pub fn as_str(&self) -> &'static str {
        match self {
            SequenceEnergy::Composed => "composed",
            SequenceEnergy::Elevated => "elevated",
            SequenceEnergy::Kinetic => "kinetic",
            SequenceEnergy::Violent => "violent",
        }
    }

    pub fn parse(value: &str) -> Option<SequenceEnergy> {
        match value {
            "composed" => Some(SequenceEnergy::Composed),
            "elevated" => Some(SequenceEnergy::Elevated),
            "kinetic" => Some(SequenceEnergy::Kinetic),
            "violent" => Some(SequenceEnergy::Violent),
            _ => None,
        }
    }
}

/// The two deliberate continuation directions for extending a canonical take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionDirection {
    Prequel,
    Sequel,
}

impl ExtensionDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExtensionDirection::Prequel => "prequel",
            ExtensionDirection::Sequel => "sequel",
        }
    }

    pub fn parse(value: &str) -> Option<ExtensionDirection> {
        match value {
            "prequel" => Some(ExtensionDirection::Prequel),
            "sequel" => Some(ExtensionDirection::Sequel),
            _ => None,
        }
    }
}

/// Caller-supplied director brief (command boundary). Validated before any
/// write happens; nothing is persisted for a rejected brief.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceBriefInput {
    pub intent: String,
    pub energy: String,
    pub target_duration_seconds: Option<f64>,
    pub credit_cap: i64,
}

/// The persisted, human-authored director brief.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceBriefRecord {
    pub intent: String,
    pub energy: SequenceEnergy,
    pub target_duration_seconds: Option<f64>,
    pub credit_cap: i64,
}

/// The persisted sequence-flow record for one scene.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceFlowRecord {
    pub scene_id: String,
    pub brief: SequenceBriefRecord,
    pub stage: SequenceStage,
    pub approved_compilation_id: Option<String>,
    pub canonical_shot_id: Option<String>,
    pub extension_direction: Option<ExtensionDirection>,
    pub created_at: String,
    pub updated_at: String,
}

/// A production rule or missing continuity anchor that blocks the flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceBlockerRecord {
    pub code: String,
    pub message: String,
}

/// Result of the explicit "references ready" action: either the blockers that
/// keep the sequence blocked (with no mutation), or the advanced flow.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferencesReadyReport {
    pub flow: Option<SequenceFlowRecord>,
    pub blockers: Vec<SequenceBlockerRecord>,
}

/// The explicit, inspectable input for extending the exact canonical video of
/// a shot in a chosen direction. Preparation only: no provider work is
/// enqueued by creating this request.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionRequestRecord {
    pub scene_id: String,
    pub shot_id: String,
    pub direction: ExtensionDirection,
    pub canonical_video_asset_version_id: String,
    pub carried_locks: BehavioralLocks,
    pub world_continuity: WorldContinuity,
    pub continuation_prompt: String,
}

/// The sequence-flow application service. All stage mutations funnel through
/// [`SequenceFlowService::transition`]'s compare-and-set.
pub struct SequenceFlowService;

/// Runtime bounds mirrored from the domain contract (1–120 seconds).
const TARGET_DURATION_MAX_SECONDS: f64 = 120.0;
const BRIEF_INTENT_MAX_CHARS: usize = 1000;

fn parse_stage(value: &str) -> Result<SequenceStage, AppError> {
    match value {
        "draft" => Ok(SequenceStage::Draft),
        "brief_locked" => Ok(SequenceStage::BriefLocked),
        "references_ready" => Ok(SequenceStage::ReferencesReady),
        "prompt_approved" => Ok(SequenceStage::PromptApproved),
        "generating" => Ok(SequenceStage::Generating),
        "in_review" => Ok(SequenceStage::InReview),
        "canonical_selected" => Ok(SequenceStage::CanonicalSelected),
        "ready_for_edit" => Ok(SequenceStage::ReadyForEdit),
        other => Err(AppError::Database(format!(
            "stored sequence stage {other:?} is not a known stage"
        ))),
    }
}

impl SequenceFlowService {
    /// Reads one flow row. Reports [`AppError::SequenceFlowNotFound`] when
    /// the scene has not started a flow yet.
    fn read_flow(conn: &Connection, scene_id: &str) -> Result<SequenceFlowRecord, AppError> {
        let row = conn
            .query_row(
                "SELECT scene_id, brief_json, stage, approved_compilation_id, \
                 canonical_shot_id, extension_direction, created_at, updated_at \
                 FROM sequence_flows WHERE scene_id = ?1",
                params![scene_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                },
            )
            .optional()
            .map_err(|e| AppError::Database(e.to_string()))?;

        let Some((scene_id, brief_json, stage, approved_compilation_id, canonical_shot_id, extension_direction, created_at, updated_at)) =
            row
        else {
            return Err(AppError::SequenceFlowNotFound);
        };

        let brief: SequenceBriefRecord = serde_json::from_str(&brief_json).map_err(|e| {
            AppError::Database(format!("stored sequence brief is not valid JSON: {e}"))
        })?;
        let stage = parse_stage(&stage)?;
        let extension_direction = extension_direction
            .map(|value| {
                ExtensionDirection::parse(&value).ok_or_else(|| {
                    AppError::Database(format!(
                        "stored extension direction {value:?} is not a known direction"
                    ))
                })
            })
            .transpose()?;

        Ok(SequenceFlowRecord {
            scene_id,
            brief,
            stage,
            approved_compilation_id,
            canonical_shot_id,
            extension_direction,
            created_at,
            updated_at,
        })
    }

    /// Validates the director brief completely before any write happens.
    fn validate_brief(input: &SequenceBriefInput) -> Result<SequenceBriefRecord, AppError> {
        let intent = input.intent.trim();
        if intent.is_empty() || intent.chars().count() > BRIEF_INTENT_MAX_CHARS {
            return Err(AppError::InvalidSequenceBrief);
        }
        let energy =
            SequenceEnergy::parse(&input.energy).ok_or(AppError::InvalidSequenceBrief)?;
        if let Some(duration) = input.target_duration_seconds {
            if !duration.is_finite()
                || duration <= 0.0
                || duration > TARGET_DURATION_MAX_SECONDS
            {
                return Err(AppError::InvalidSequenceBrief);
            }
        }
        if input.credit_cap < 0 {
            return Err(AppError::InvalidSequenceBrief);
        }
        Ok(SequenceBriefRecord {
            intent: intent.to_string(),
            energy,
            target_duration_seconds: input.target_duration_seconds,
            credit_cap: input.credit_cap,
        })
    }

    /// The single guarded stage mutation: `next` must be the only legal
    /// successor of `expected`, and the row must still be at `expected`.
    /// A concurrent change loses with [`AppError::SequenceFlowStageConflict`]
    /// and the row is left untouched.
    pub fn transition(
        conn: &Connection,
        scene_id: &str,
        expected: SequenceStage,
        next: SequenceStage,
    ) -> Result<SequenceFlowRecord, AppError> {
        if Some(next) != expected.successor() {
            return Err(AppError::WorkflowInvalidTransition(format!(
                "a sequence flow cannot move from {} to {}",
                expected.as_str(),
                next.as_str()
            )));
        }
        let updated = conn
            .execute(
                "UPDATE sequence_flows SET stage = ?1, updated_at = ?2 \
                 WHERE scene_id = ?3 AND stage = ?4",
                params![next.as_str(), Utc::now().to_rfc3339(), scene_id, expected.as_str()],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        if updated == 0 {
            return Err(AppError::SequenceFlowStageConflict);
        }
        Self::read_flow(conn, scene_id)
    }

    /// Reads one flow for an opened project. Scenes of other projects are
    /// reported as missing, never leaked.
    pub fn get_flow(project_root: &Path, scene_id: &str) -> Result<SequenceFlowRecord, AppError> {
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        cinema_repository::ensure_scene_in_project(&conn, &project.id, scene_id)?;
        Self::read_flow(&conn, scene_id)
    }

    /// Saves (and on first save, locks) the human-authored director brief.
    /// The brief is validated before anything is written; once the flow has
    /// moved past `brief_locked` the brief can no longer be edited.
    pub fn update_brief(
        project_root: &Path,
        scene_id: &str,
        input: &SequenceBriefInput,
    ) -> Result<SequenceFlowRecord, AppError> {
        let brief = Self::validate_brief(input)?;
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        cinema_repository::ensure_scene_in_project(&conn, &project.id, scene_id)?;

        let brief_json =
            serde_json::to_string(&brief).map_err(|e| AppError::Database(e.to_string()))?;
        let existing = match Self::read_flow(&conn, scene_id) {
            Ok(flow) => Some(flow),
            Err(AppError::SequenceFlowNotFound) => None,
            Err(error) => return Err(error),
        };
        let now = Utc::now().to_rfc3339();
        match existing {
            None => {
                conn.execute(
                    "INSERT INTO sequence_flows (scene_id, brief_json, stage, \
                     approved_compilation_id, canonical_shot_id, extension_direction, \
                     created_at, updated_at) \
                     VALUES (?1, ?2, 'brief_locked', NULL, NULL, NULL, ?3, ?3)",
                    params![scene_id, brief_json, now],
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
            }
            Some(flow) => {
                if flow.stage != SequenceStage::BriefLocked {
                    return Err(AppError::WorkflowInvalidTransition(format!(
                        "the director brief can only be edited while the sequence \
                         is at brief_locked, not {}",
                        flow.stage.as_str()
                    )));
                }
                conn.execute(
                    "UPDATE sequence_flows SET brief_json = ?1, updated_at = ?2 \
                     WHERE scene_id = ?3",
                    params![brief_json, now, scene_id],
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
            }
        }
        Self::read_flow(&conn, scene_id)
    }

    /// The explicit "references are ready" action. When continuity anchors
    /// are missing, the blockers are reported and nothing is mutated.
    pub fn mark_references_ready(
        project_root: &Path,
        scene_id: &str,
    ) -> Result<ReferencesReadyReport, AppError> {
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        cinema_repository::ensure_scene_in_project(&conn, &project.id, scene_id)?;
        Self::read_flow(&conn, scene_id)?;

        let mut blockers: Vec<SequenceBlockerRecord> = Vec::new();

        let world: Option<(Option<String>, Option<String>)> = conn
            .query_row(
                "SELECT ws.world_id, plate.canonical_version_id \
                 FROM world_scenes ws \
                 LEFT JOIN worlds w ON w.id = ws.world_id \
                 LEFT JOIN assets plate ON plate.id = w.world_plate_asset_id \
                 WHERE ws.id = ?1",
                params![scene_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|e| AppError::Database(e.to_string()))?;
        match world {
            Some((Some(_), Some(_))) => {}
            _ => blockers.push(SequenceBlockerRecord {
                code: "world_reference_missing".to_string(),
                message: "Missing scene plate: assign a World whose plate has a canonical version"
                    .to_string(),
            }),
        }

        let cast: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM world_scene_characters WHERE scene_id = ?1",
                params![scene_id],
                |row| row.get(0),
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        if cast == 0 {
            blockers.push(SequenceBlockerRecord {
                code: "no_cast".to_string(),
                message: "No cast: add at least one character with a canonical look".to_string(),
            });
        }

        let shots: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM scene_shots WHERE scene_id = ?1",
                params![scene_id],
                |row| row.get(0),
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        if shots == 0 {
            blockers.push(SequenceBlockerRecord {
                code: "no_shots".to_string(),
                message: "No shots: add at least one shot to the sequence".to_string(),
            });
        }

        if !blockers.is_empty() {
            return Ok(ReferencesReadyReport {
                flow: None,
                blockers,
            });
        }

        let flow = Self::transition(
            &conn,
            scene_id,
            SequenceStage::BriefLocked,
            SequenceStage::ReferencesReady,
        )?;
        Ok(ReferencesReadyReport {
            flow: Some(flow),
            blockers: Vec::new(),
        })
    }

    /// The explicit generation approval. The compare-and-set only accepts a
    /// flow at `references_ready`; a concurrent stage change loses cleanly
    /// without touching the row. Recorded in a single transaction with the
    /// stage move so the approval and its compilation reference are atomic.
    pub fn approve_preflight(
        project_root: &Path,
        scene_id: &str,
        approved_compilation_id: Option<String>,
    ) -> Result<SequenceFlowRecord, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        cinema_repository::ensure_scene_in_project(&conn, &project.id, scene_id)?;
        Self::read_flow(&conn, scene_id)?;

        let tx = conn
            .transaction()
            .map_err(|e| AppError::Database(e.to_string()))?;
        Self::transition(
            &tx,
            scene_id,
            SequenceStage::ReferencesReady,
            SequenceStage::PromptApproved,
        )?;
        tx.execute(
            "UPDATE sequence_flows SET approved_compilation_id = ?1 WHERE scene_id = ?2",
            params![approved_compilation_id, scene_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        tx.commit()
            .map_err(|e| AppError::Database(e.to_string()))?;
        Self::read_flow(&conn, scene_id)
    }

    /// Moves a generating sequence into human review. Only a flow currently
    /// at `generating` may advance.
    pub fn begin_review(
        project_root: &Path,
        scene_id: &str,
    ) -> Result<SequenceFlowRecord, AppError> {
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        cinema_repository::ensure_scene_in_project(&conn, &project.id, scene_id)?;
        Self::read_flow(&conn, scene_id)?;
        Self::transition(
            &conn,
            scene_id,
            SequenceStage::Generating,
            SequenceStage::InReview,
        )
    }

    /// Prepares (never executes) the extension of the scene's exact canonical
    /// video: resolves the pinned version, carries the scene's locked
    /// behavioral and world continuity, and returns a disclosure object. A
    /// scene without a canonical video pin cannot be extended.
    pub fn prepare_extension(
        project_root: &Path,
        scene_id: &str,
        direction: &str,
    ) -> Result<ExtensionRequestRecord, AppError> {
        let direction = ExtensionDirection::parse(direction)
            .ok_or(AppError::InvalidSequenceExtensionDirection)?;
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        cinema_repository::ensure_scene_in_project(&conn, &project.id, scene_id)?;

        // The extension source is the shot's exact canonical video pin (the
        // same reference `resolve_canonical_shot_video` resolves). The latest
        // pinned shot wins; a scene with no pin has nothing to extend.
        let shots = cinema_repository::list_shots(&conn, scene_id)?;
        let mut canonical: Option<(String, String)> = None;
        for shot in &shots {
            if let Some(version_id) = resolve_canonical_video_version(&conn, &shot.id)? {
                canonical = Some((shot.id.clone(), version_id));
            }
        }
        let Some((shot_id, version_id)) = canonical else {
            return Err(AppError::SequenceCanonicalVideoMissing);
        };

        // Behavioral canon is a disclosure input, not a gate here: a cast
        // member with unlocked canon degrades to no carried lock rather than
        // blocking the inspection of the prepared request.
        let carried_locks = match CinemaService::resolve_scene_behavioral_locks(&conn, scene_id) {
            Ok(locks) => locks,
            Err(AppError::WorkflowPrerequisiteFailed(_)) => BehavioralLocks::default(),
            Err(error) => return Err(error),
        };

        let world_version: Option<Option<String>> = conn
            .query_row(
                "SELECT world_asset_version_id FROM world_scenes WHERE id = ?1",
                params![scene_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let world_continuity = match world_version.flatten() {
            Some(version_id) => {
                match ensure_canonical_version(&conn, &project.id, &version_id, &["world_plate"]) {
                    Ok(plate) => WorldContinuity {
                        plate_id: Some(plate.asset_id),
                        plate_asset_version_id: Some(plate.version_id),
                        description: Some(plate.label),
                    },
                    Err(_) => WorldContinuity::default(),
                }
            }
            None => WorldContinuity::default(),
        };

        let continuation_prompt = format!(
            "Extend the exact canonical take of shot {shot_id} as its {}: keep the \
             scene's locked looks, world plate continuity, and behavioral canon unchanged.",
            direction.as_str()
        );

        // Record the chosen extension target on the flow when one exists.
        // This does not change the stage and does not enqueue provider work.
        conn.execute(
            "UPDATE sequence_flows SET canonical_shot_id = ?1, extension_direction = ?2, \
             updated_at = ?3 WHERE scene_id = ?4",
            params![shot_id, direction.as_str(), Utc::now().to_rfc3339(), scene_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(ExtensionRequestRecord {
            scene_id: scene_id.to_string(),
            shot_id,
            direction,
            canonical_video_asset_version_id: version_id,
            carried_locks,
            world_continuity,
            continuation_prompt,
        })
    }
}
