//! Shot video review domain decisions (P10.4).
//!
//! Review state and canonical state are two orthogonal dimensions:
//!
//!   * review — `Active` / `Rejected`, changed only by explicit human
//!     Reject/Restore actions, never by generation or QA;
//!   * canonical — derived from the Shot's exact pinned video version,
//!     changed only by the P10.2 promotion primitive plus the P10.4
//!     QA/staleness override gate.
//!
//! These functions are the single source of truth for the invariants:
//! a rejected candidate cannot be promoted, the canonical candidate
//! cannot be rejected, rejection is reversible, promoting the current
//! canonical is idempotent, and exceptional (QA-failed / stale) candidates
//! demand an explicit non-empty override reason before promotion.

use crate::error::AppError;

/// Review state of one video candidate. Rejected means hidden/de-emphasized
/// and non-promotable; artifacts and QA records remain intact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CandidateReviewState {
    Active,
    Rejected,
}

impl CandidateReviewState {
    pub fn as_str(self) -> &'static str {
        match self {
            CandidateReviewState::Active => "active",
            CandidateReviewState::Rejected => "rejected",
        }
    }
}

/// Why a candidate would be exceptional to promote (advisory QA / stale
/// inputs). Surfaced to the human; never blocks a legitimate decision, but
/// demands an explicit acknowledgement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionRisk {
    pub qa_overall_status: Option<String>,
    pub source_shot_version_current: bool,
    pub source_keyframe_is_current_pin: bool,
}

impl PromotionRisk {
    /// True when the candidate carries a production risk a human must
    /// explicitly acknowledge: failed or needs-review QA, or stale inputs.
    /// QA never having run is NOT a risk — a fresh candidate is normal.
    pub fn is_exceptional(&self) -> bool {
        let qa_is_risky = matches!(
            self.qa_overall_status.as_deref(),
            Some("fail") | Some("needs_review")
        );
        qa_is_risky || !self.source_shot_version_current || !self.source_keyframe_is_current_pin
    }
}

/// Validated promotion decision for one candidate.
///
/// `unchanged` means the candidate is already the Shot's canonical video:
/// the promotion use case must succeed as a no-op without appending a
/// duplicate audit event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionDecision {
    pub candidate_version_id: String,
    pub previous_version_id: Option<String>,
    pub qa_override: bool,
    pub unchanged: bool,
}

/// Decides whether `candidate` may be promoted to become the Shot's
/// canonical video, replacing `canonical_version_id` (None when the Shot
/// has no canonical video yet).
///
/// Errors:
///   * `CandidateRejected` — a rejected candidate cannot be promoted;
///   * `CanonicalConflict`-shaped `PromotionConflict` — ownership/
///     concurrency problems (candidate from another Shot, stale expected
///     canonical) are decided by the transactional caller; this function
///     enforces only the state-based rules below.
pub fn decide_promotion(
    candidate_version_id: &str,
    candidate_review: CandidateReviewState,
    candidate_belongs_to_shot: bool,
    canonical_version_id: Option<&str>,
    risk: &PromotionRisk,
    override_reason: Option<&str>,
) -> Result<PromotionDecision, AppError> {
    if !candidate_belongs_to_shot {
        return Err(AppError::GenerationArtifactNotPromotable);
    }
    if candidate_review == CandidateReviewState::Rejected {
        return Err(AppError::CandidateRejected);
    }
    if canonical_version_id == Some(candidate_version_id) {
        // Idempotent re-promotion of the current canonical: success, no
        // new history, no override demand.
        return Ok(PromotionDecision {
            candidate_version_id: candidate_version_id.to_string(),
            previous_version_id: canonical_version_id.map(str::to_string),
            qa_override: false,
            unchanged: true,
        });
    }
    if risk.is_exceptional() && reason_is_missing(override_reason) {
        return Err(AppError::QaOverrideRequired);
    }
    Ok(PromotionDecision {
        candidate_version_id: candidate_version_id.to_string(),
        previous_version_id: canonical_version_id.map(str::to_string),
        qa_override: risk.is_exceptional(),
        unchanged: false,
    })
}

/// Decides whether `candidate_version_id` may be rejected.
///
/// The current canonical video cannot be rejected: rejecting it would
/// create contradictory production state. Rejection must never silently
/// clear the canonical selection.
pub fn decide_rejection(
    candidate_version_id: &str,
    canonical_version_id: Option<&str>,
    current_review: CandidateReviewState,
) -> Result<(), AppError> {
    if canonical_version_id == Some(candidate_version_id) {
        return Err(AppError::CanonicalCandidateCannotBeRejected);
    }
    let _ = current_review; // rejecting an already-rejected candidate is a no-op
    Ok(())
}

/// Restoring a rejected candidate returns it to Active and never promotes
/// it; restoring an active candidate is a no-op.
pub fn decide_restoration(current_review: CandidateReviewState) -> CandidateReviewState {
    match current_review {
        CandidateReviewState::Active | CandidateReviewState::Rejected => {
            CandidateReviewState::Active
        }
    }
}

fn reason_is_missing(reason: Option<&str>) -> bool {
    match reason {
        None => true,
        Some(reason) => reason.trim().is_empty(),
    }
}

/// Persistence for candidate review state (P10.4). Review rows live in
/// `shot_video_review_states`; absence of a row means `Active`. Canonical
/// selection is NOT stored here — it stays on the Shot pin (P10.2).
pub mod repository {
    use super::CandidateReviewState;
    use crate::error::AppError;
    use rusqlite::{params, OptionalExtension};

    fn row_to_state(state: String) -> CandidateReviewState {
        match state.as_str() {
            "rejected" => CandidateReviewState::Rejected,
            _ => CandidateReviewState::Active,
        }
    }

    /// Reads the review state of one video candidate version. A missing
    /// row means the candidate is Active (generation and QA never write
    /// review rows).
    pub fn review_state(
        conn: &rusqlite::Connection,
        asset_version_id: &str,
    ) -> Result<CandidateReviewState, AppError> {
        let state: Option<String> = conn
            .query_row(
                "SELECT state FROM shot_video_review_states WHERE asset_version_id = ?1",
                params![asset_version_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| AppError::Database(error.to_string()))?;
        Ok(state
            .map(row_to_state)
            .unwrap_or(CandidateReviewState::Active))
    }

    /// Persists `Rejected` for one candidate. Idempotent: an already
    /// rejected candidate keeps its original rejection record.
    pub fn reject_candidate(
        conn: &rusqlite::Connection,
        project_id: &str,
        asset_version_id: &str,
        reason: Option<&str>,
    ) -> Result<(), AppError> {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO shot_video_review_states \
             (asset_version_id, project_id, state, reason, created_at, updated_at) \
             VALUES (?1, ?2, 'rejected', ?3, ?4, ?4) \
             ON CONFLICT(asset_version_id) DO UPDATE SET \
             state = 'rejected', updated_at = excluded.updated_at",
            params![asset_version_id, project_id, reason, now],
        )
        .map_err(|error| AppError::Database(error.to_string()))?;
        Ok(())
    }

    /// Restores a rejected candidate to Active by deleting its review row,
    /// returning the candidate to the default Active state. Idempotent for
    /// candidates that were never rejected.
    pub fn restore_candidate(
        conn: &rusqlite::Connection,
        asset_version_id: &str,
    ) -> Result<(), AppError> {
        conn.execute(
            "DELETE FROM shot_video_review_states WHERE asset_version_id = ?1",
            params![asset_version_id],
        )
        .map_err(|error| AppError::Database(error.to_string()))?;
        Ok(())
    }
}

/// Application-level read model for the Shot video review UI (P10.4).
///
/// One `ShotVideoCandidate` row per successful shot-video candidate of the
/// Shot: every video AssetVersion of the Shot's scene-owned video asset
/// that was produced by a `shot.image_to_video` run naming this exact Shot.
/// Failed attempts without a usable artifact never become versions, so they
/// never appear as candidates. QA data is reused from `qa_runs` (P10.3) —
/// never recalculated.
pub mod read_model {
    use super::CandidateReviewState;
    use crate::error::AppError;
    use rusqlite::{params, OptionalExtension};
    use serde::{Deserialize, Serialize};

    /// One reviewable video candidate of a Shot. All fields the review UI
    /// needs, resolved below the React layer.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ShotVideoCandidate {
        pub asset_version_id: String,
        pub version_number: i64,
        pub shot_id: String,
        pub scene_id: String,
        pub created_at: String,
        pub file_path: String,
        pub mime_type: String,
        pub byte_size: i64,
        pub review_state: CandidateReviewState,
        pub is_canonical: bool,
        /// Latest completed video QA overall status for this exact version
        /// (`pass` | `fail` | `needs_review`), or None when QA never ran.
        pub qa_overall_status: Option<String>,
        pub qa_run_count: i64,
        /// Provenance from the producing run / lineage.
        pub provider_id: Option<String>,
        pub model_id: Option<String>,
        pub workflow_run_id: Option<String>,
        /// The exact frozen keyframe version used as the I2V source.
        pub source_asset_version_id: Option<String>,
        /// True when the candidate's frozen source keyframe is still the
        /// Shot's current keyframe pin (i.e. inputs have not drifted).
        pub source_keyframe_is_current: bool,
    }

    /// Lists all successful video candidates of one Shot, newest first.
    /// `canonical_asset_version_id` is the Shot's exact pinned video
    /// version (None = no canonical selection).
    pub fn list_shot_video_candidates(
        conn: &rusqlite::Connection,
        shot_id: &str,
        canonical_asset_version_id: Option<&str>,
    ) -> Result<Vec<ShotVideoCandidate>, AppError> {
        let (scene_id, keyframe_pin): (String, Option<String>) = conn
            .query_row(
                "SELECT ss.scene_id, ss.keyframe_asset_version_id \
                 FROM scene_shots ss \
                 JOIN world_scenes ws ON ws.id = ss.scene_id \
                 WHERE ss.id = ?1",
                params![shot_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| AppError::Database(error.to_string()))?
            .ok_or(AppError::ShotNotFound)?;

        // Every candidate video version produced by a shot.image_to_video
        // run frozen for this exact Shot. A version belongs to the Shot
        // through its producing run's frozen input (shotId), which the
        // promotion path also treats as ownership. The artifact→version
        // pairing is by exact sha256 (import dedup keeps this 1:1 for a
        // scene video asset); promotion history (artifact_promotions) is
        // NOT required, so uncaptured-into-history candidates appear too.
        let mut stmt = conn
            .prepare(
                "SELECT av.id, av.version_number, av.created_at, av.file_path, \
                 av.mime_type, av.byte_size, \
                 rv.state, \
                 (SELECT overall_status FROM qa_runs \
                   WHERE asset_version_id = av.id AND overall_status IS NOT NULL \
                     AND json_extract(check_plan_json, '$.assetType') = 'video' \
                   ORDER BY created_at DESC, id DESC LIMIT 1) AS qa_overall, \
                 (SELECT COUNT(*) FROM qa_runs \
                   WHERE asset_version_id = av.id AND json_extract(check_plan_json, '$.assetType') = 'video') AS qa_count, \
                 al.workflow_run_id, al.provider_id, al.model_id, \
                 (SELECT asset_version_id FROM generated_artifact_sources \
                   WHERE artifact_id = al.artifact_id \
                   ORDER BY ordinal ASC LIMIT 1) AS source_version \
                 FROM asset_versions av \
                 JOIN assets a ON a.id = av.asset_id \
                 JOIN generated_artifacts ga ON ga.sha256 = av.sha256 \
                 JOIN artifact_lineage al ON al.artifact_id = ga.id \
                 JOIN workflow_runs wr ON wr.id = al.workflow_run_id \
                 LEFT JOIN shot_video_review_states rv ON rv.asset_version_id = av.id \
                 WHERE a.type = 'video' AND a.owner_entity_id = ?1 \
                   AND wr.operation_id = 'shot.image_to_video' \
                   AND json_extract(wr.input_json, '$.shotId') = ?2 \
                 ORDER BY av.created_at DESC, av.version_number DESC",
            )
            .map_err(|error| AppError::Database(error.to_string()))?;

        let rows = stmt
            .query_map(params![scene_id, shot_id], |row| {
                Ok(ShotVideoCandidate {
                    asset_version_id: row.get(0)?,
                    version_number: row.get(1)?,
                    shot_id: shot_id.to_string(),
                    scene_id: scene_id.clone(),
                    created_at: row.get(2)?,
                    file_path: row.get(3)?,
                    mime_type: row.get(4)?,
                    byte_size: row.get(5)?,
                    review_state: match row.get::<_, Option<String>>(6)? {
                        Some(ref state) if state == "rejected" => CandidateReviewState::Rejected,
                        _ => CandidateReviewState::Active,
                    },
                    is_canonical: canonical_asset_version_id == Some(&row.get::<_, String>(0)?),
                    qa_overall_status: row.get(7)?,
                    qa_run_count: row.get(8)?,
                    workflow_run_id: row.get(9)?,
                    provider_id: row.get(10)?,
                    model_id: row.get(11)?,
                    source_asset_version_id: row.get(12)?,
                    source_keyframe_is_current: match row.get::<_, Option<String>>(12)? {
                        Some(source) => Some(source) == keyframe_pin,
                        None => false,
                    },
                })
            })
            .map_err(|error| AppError::Database(error.to_string()))?;

        let mut candidates = Vec::new();
        for row in rows {
            candidates.push(row.map_err(|error| AppError::Database(error.to_string()))?);
        }
        Ok(candidates)
    }

    /// Resolves the Shot's canonical video: the exact pinned version, or
    /// `None` when no human has promoted one. There is deliberately NO
    /// fallback to the latest successful generation (P10.4 §5).
    pub fn resolve_canonical_video_version(
        conn: &rusqlite::Connection,
        shot_id: &str,
    ) -> Result<Option<String>, AppError> {
        conn.query_row(
            "SELECT ss.generated_video_asset_version_id \
             FROM scene_shots ss \
             JOIN world_scenes ws ON ws.id = ss.scene_id \
             WHERE ss.id = ?1",
            params![shot_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|error| AppError::Database(error.to_string()))
        .map(|row| row.flatten())
    }
}

/// Human review actions at the application boundary (P10.4): Reject and
/// Restore. Invariants are enforced here (Rust), never in React:
///   * the current canonical video cannot be rejected;
///   * rejection is reversible and never deletes artifacts or QA history;
///   * restoring returns a candidate to Active without promoting it.
pub mod service {
    use super::read_model::resolve_canonical_video_version;
    use super::repository::{reject_candidate, restore_candidate, review_state};
    use super::{decide_rejection, decide_restoration, CandidateReviewState};
    use crate::error::AppError;
    use std::path::Path;

    /// Rejects one shot video candidate for review. Idempotent for
    /// already-rejected candidates; errors for the canonical video.
    pub fn reject_shot_video_candidate(
        project_root: &Path,
        shot_id: &str,
        asset_version_id: &str,
        reason: Option<&str>,
    ) -> Result<CandidateReviewState, AppError> {
        let conn = crate::db::open_existing_connection(&project_root.join("project.db"))?;
        let project_id: String = conn
            .query_row("SELECT id FROM projects", [], |row| row.get(0))
            .map_err(|error| AppError::Database(error.to_string()))?;

        // The candidate must exist, be a video version, and belong to the
        // Shot's scene-owned video asset (ownership via the candidate list).
        let candidates = super::read_model::list_shot_video_candidates(
            &conn,
            shot_id,
            resolve_canonical_video_version(&conn, shot_id)?.as_deref(),
        )?;
        let candidate = candidates
            .iter()
            .find(|candidate| candidate.asset_version_id == asset_version_id)
            .ok_or(AppError::AssetVersionNotFound)?;

        // Domain decision: canonical candidates cannot be rejected.
        decide_rejection(
            asset_version_id,
            candidate.is_canonical.then_some(asset_version_id),
            review_state(&conn, asset_version_id)?,
        )?;

        reject_candidate(&conn, &project_id, asset_version_id, reason)?;
        Ok(CandidateReviewState::Rejected)
    }

    /// Restores a rejected shot video candidate to Active. Idempotent for
    /// active candidates; never promotes.
    pub fn restore_shot_video_candidate(
        project_root: &Path,
        shot_id: &str,
        asset_version_id: &str,
    ) -> Result<CandidateReviewState, AppError> {
        let conn = crate::db::open_existing_connection(&project_root.join("project.db"))?;
        let candidates = super::read_model::list_shot_video_candidates(
            &conn,
            shot_id,
            resolve_canonical_video_version(&conn, shot_id)?.as_deref(),
        )?;
        if !candidates
            .iter()
            .any(|candidate| candidate.asset_version_id == asset_version_id)
        {
            return Err(AppError::AssetVersionNotFound);
        }
        let restored = decide_restoration(review_state(&conn, asset_version_id)?);
        restore_candidate(&conn, asset_version_id)?;
        Ok(restored)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const V1: &str = "ver-1";
    const V2: &str = "ver-2";
    const V3: &str = "ver-3";

    fn ok_risk() -> PromotionRisk {
        PromotionRisk {
            qa_overall_status: Some("pass".into()),
            source_shot_version_current: true,
            source_keyframe_is_current_pin: true,
        }
    }

    fn failed_qa_risk() -> PromotionRisk {
        PromotionRisk {
            qa_overall_status: Some("fail".into()),
            ..ok_risk()
        }
    }

    fn stale_risk() -> PromotionRisk {
        PromotionRisk {
            source_shot_version_current: false,
            ..ok_risk()
        }
    }

    #[test]
    fn active_candidate_of_the_shot_can_be_promoted() {
        let decision = decide_promotion(
            V2,
            CandidateReviewState::Active,
            true,
            Some(V1),
            &ok_risk(),
            None,
        )
        .unwrap();
        assert_eq!(decision.candidate_version_id, V2);
        assert_eq!(decision.previous_version_id, Some(V1.to_string()));
        assert!(!decision.qa_override);
        assert!(!decision.unchanged);
    }

    #[test]
    fn rejected_candidate_cannot_be_promoted() {
        let error = decide_promotion(
            V2,
            CandidateReviewState::Rejected,
            true,
            Some(V1),
            &ok_risk(),
            None,
        )
        .unwrap_err();
        assert_eq!(error.code(), "CANDIDATE_REJECTED");
    }

    #[test]
    fn candidate_from_another_shot_cannot_be_promoted() {
        let error = decide_promotion(
            V2,
            CandidateReviewState::Active,
            false,
            None,
            &ok_risk(),
            None,
        )
        .unwrap_err();
        assert_eq!(error.code(), "GENERATION_ARTIFACT_NOT_PROMOTABLE");
    }

    #[test]
    fn canonical_candidate_cannot_be_rejected() {
        let error = decide_rejection(V1, Some(V1), CandidateReviewState::Active).unwrap_err();
        assert_eq!(error.code(), "CANONICAL_CANDIDATE_CANNOT_BE_REJECTED");
    }

    #[test]
    fn noncanonical_candidate_can_be_rejected() {
        decide_rejection(V2, Some(V1), CandidateReviewState::Active).unwrap();
    }

    #[test]
    fn rejection_is_reversible_without_promotion() {
        let restored = decide_restoration(CandidateReviewState::Rejected);
        assert_eq!(restored, CandidateReviewState::Active);
    }

    #[test]
    fn promoting_the_current_canonical_is_an_idempotent_noop() {
        let decision = decide_promotion(
            V1,
            CandidateReviewState::Active,
            true,
            Some(V1),
            &ok_risk(),
            None,
        )
        .unwrap();
        assert!(decision.unchanged);
        assert_eq!(decision.previous_version_id, Some(V1.to_string()));
        assert!(!decision.qa_override);
    }

    #[test]
    fn idempotent_promotion_holds_even_for_exceptional_canonical() {
        // The canonical candidate may itself be exceptional (it was
        // promoted with an override); re-promoting it stays a no-op.
        let decision = decide_promotion(
            V1,
            CandidateReviewState::Active,
            true,
            Some(V1),
            &failed_qa_risk(),
            None,
        )
        .unwrap();
        assert!(decision.unchanged);
    }

    #[test]
    fn promoting_a_different_candidate_reports_the_transition() {
        let decision = decide_promotion(
            V3,
            CandidateReviewState::Active,
            true,
            Some(V2),
            &ok_risk(),
            None,
        )
        .unwrap();
        assert_eq!(decision.previous_version_id, Some(V2.to_string()));
        assert_eq!(decision.candidate_version_id, V3);
    }

    #[test]
    fn first_promotion_reports_a_null_previous_canonical() {
        let decision = decide_promotion(
            V1,
            CandidateReviewState::Active,
            true,
            None,
            &ok_risk(),
            None,
        )
        .unwrap();
        assert_eq!(decision.previous_version_id, None);
    }

    #[test]
    fn qa_failed_candidate_demands_an_explicit_override() {
        let error = decide_promotion(
            V2,
            CandidateReviewState::Active,
            true,
            Some(V1),
            &failed_qa_risk(),
            None,
        )
        .unwrap_err();
        assert_eq!(error.code(), "QA_OVERRIDE_REQUIRED");
    }

    #[test]
    fn stale_source_candidate_demands_an_explicit_override() {
        let error = decide_promotion(
            V2,
            CandidateReviewState::Active,
            true,
            Some(V1),
            &stale_risk(),
            None,
        )
        .unwrap_err();
        assert_eq!(error.code(), "QA_OVERRIDE_REQUIRED");
    }

    #[test]
    fn explicit_override_promotes_the_exceptional_candidate() {
        let decision = decide_promotion(
            V2,
            CandidateReviewState::Active,
            true,
            Some(V1),
            &failed_qa_risk(),
            Some("Director approved this take despite the QA warning."),
        )
        .unwrap();
        assert!(decision.qa_override);
        assert!(!decision.unchanged);
        assert_eq!(decision.previous_version_id, Some(V1.to_string()));
    }

    #[test]
    fn whitespace_only_override_reason_is_rejected() {
        let error = decide_promotion(
            V2,
            CandidateReviewState::Active,
            true,
            Some(V1),
            &failed_qa_risk(),
            Some("   "),
        )
        .unwrap_err();
        assert_eq!(error.code(), "QA_OVERRIDE_REQUIRED");
    }

    #[test]
    fn normal_promotion_does_not_record_a_qa_override() {
        let decision = decide_promotion(
            V2,
            CandidateReviewState::Active,
            true,
            Some(V1),
            &ok_risk(),
            Some("unneeded reason"),
        )
        .unwrap();
        assert!(!decision.qa_override);
    }

    #[test]
    fn needs_review_qa_is_exceptional() {
        let risk = PromotionRisk {
            qa_overall_status: Some("needs_review".into()),
            ..ok_risk()
        };
        let error = decide_promotion(
            V2,
            CandidateReviewState::Active,
            true,
            Some(V1),
            &risk,
            None,
        )
        .unwrap_err();
        assert_eq!(error.code(), "QA_OVERRIDE_REQUIRED");
    }

    #[test]
    fn drifted_keyframe_pin_is_exceptional() {
        let risk = PromotionRisk {
            source_keyframe_is_current_pin: false,
            ..ok_risk()
        };
        let error = decide_promotion(
            V2,
            CandidateReviewState::Active,
            true,
            Some(V1),
            &risk,
            None,
        )
        .unwrap_err();
        assert_eq!(error.code(), "QA_OVERRIDE_REQUIRED");
    }

    // ------------------------------------------------------------------
    // Persistence (Task 2): review state survives repository reload.
    // ------------------------------------------------------------------

    mod persistence_tests {
        use super::super::repository::*;
        use super::super::CandidateReviewState;
        use crate::db::{self, migrations::run_migrations};
        use rusqlite::Connection;

        fn migrated_conn() -> Connection {
            let mut conn = Connection::open_in_memory().unwrap();
            run_migrations(&mut conn).unwrap();
            conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
            conn
        }

        /// Seeds one project + video asset + two candidate versions, and
        /// returns (conn, project_id, [version ids]).
        fn seeded() -> (Connection, String, Vec<String>) {
            let conn = migrated_conn();
            conn.execute(
                "INSERT INTO projects (id, name, created_at, updated_at, schema_version) \
                 VALUES ('p1', 'Red Door', 'now', 'now', 1)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO assets (id, project_id, type, label, owner_entity_id, \
                 canonical_version_id, created_at, updated_at) \
                 VALUES ('a-vid', 'p1', 'video', 'Scene 001 — Video', NULL, NULL, 'now', 'now')",
                [],
            )
            .unwrap();
            for (id, version_number, hash_prefix) in [("v1", 1, 'a'), ("v2", 2, 'b')] {
                conn.execute(
                    "INSERT INTO asset_versions (id, asset_id, version_number, status, \
                     file_path, thumbnail_path, sha256, original_filename, mime_type, \
                     byte_size, created_at) \
                     VALUES (?1, 'a-vid', ?2, 'candidate', 'v.mp4', '', ?3, 'v.mp4', \
                     'video/mp4', 24, 'now')",
                    rusqlite::params![id, version_number, hash_prefix.to_string().repeat(64)],
                )
                .unwrap();
            }
            (conn, "p1".into(), vec!["v1".into(), "v2".into()])
        }

        #[test]
        fn initial_candidates_are_active_without_review_rows() {
            let (conn, _, versions) = seeded();
            for version in &versions {
                assert_eq!(
                    review_state(&conn, version).unwrap(),
                    CandidateReviewState::Active
                );
            }
        }

        #[test]
        fn restore_survives_reload_and_returns_to_active() {
            let (conn, project, versions) = seeded();
            reject_candidate(&conn, &project, &versions[0], None).unwrap();
            restore_candidate(&conn, &versions[0]).unwrap();
            assert_eq!(
                review_state(&conn, &versions[0]).unwrap(),
                CandidateReviewState::Active
            );
        }

        #[test]
        fn rejecting_an_already_rejected_candidate_is_idempotent() {
            let (conn, project, versions) = seeded();
            reject_candidate(&conn, &project, &versions[0], Some("first")).unwrap();
            reject_candidate(&conn, &project, &versions[0], Some("second")).unwrap();
            let (state, reason): (String, Option<String>) = conn
                .query_row(
                    "SELECT state, reason FROM shot_video_review_states \
                     WHERE asset_version_id = ?1",
                    [&versions[0]],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(state, "rejected");
            assert_eq!(reason.as_deref(), Some("first"));
            assert_eq!(
                review_state(&conn, &versions[0]).unwrap(),
                CandidateReviewState::Rejected
            );
        }

        #[test]
        fn restoring_an_active_candidate_is_a_noop() {
            let (conn, _, versions) = seeded();
            restore_candidate(&conn, &versions[0]).unwrap();
            let rows: i64 = conn
                .query_row("SELECT COUNT(*) FROM shot_video_review_states", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(rows, 0);
        }

        #[test]
        fn invalid_cross_project_reference_is_rejected() {
            let (conn, _, versions) = seeded();
            // FK: review rows must reference a real project.
            assert!(reject_candidate(&conn, "missing-project", &versions[0], None).is_err());
        }

        #[test]
        fn unknown_versions_cannot_be_reviewed() {
            let (conn, project, _) = seeded();
            assert!(reject_candidate(&conn, &project, "missing-version", None).is_err());
        }

        #[test]
        fn deleting_a_version_cascades_its_review_row() {
            let (conn, project, versions) = seeded();
            reject_candidate(&conn, &project, &versions[0], None).unwrap();
            conn.execute("DELETE FROM asset_versions WHERE id = ?1", [&versions[0]])
                .unwrap();
            let rows: i64 = conn
                .query_row("SELECT COUNT(*) FROM shot_video_review_states", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(rows, 0);
        }

        #[test]
        fn running_migrations_twice_is_still_idempotent() {
            let mut conn = Connection::open_in_memory().unwrap();
            run_migrations(&mut conn).unwrap();
            run_migrations(&mut conn).unwrap();
        }

        #[test]
        fn review_state_survives_reopening_a_file_database() {
            let dir = tempfile::tempdir().unwrap();
            let db_path = dir.path().join("project.db");
            let project = "p1".to_string();
            let version = "v1".to_string();
            {
                let mut conn = db::open_connection(&db_path).unwrap();
                run_migrations(&mut conn).unwrap();
                conn.execute(
                    "INSERT INTO projects (id, name, created_at, updated_at, schema_version) \
                     VALUES (?1, 'Red Door', 'now', 'now', 1)",
                    [&project],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO assets (id, project_id, type, label, created_at, updated_at) \
                     VALUES ('a-vid', ?1, 'video', 'V', 'now', 'now')",
                    [&project],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO asset_versions (id, asset_id, version_number, status, \
                     file_path, thumbnail_path, sha256, original_filename, mime_type, \
                     byte_size, created_at) \
                     VALUES ('v1', 'a-vid', 1, 'candidate', 'v.mp4', '', ?, 'v.mp4', \
                     'video/mp4', 24, 'now')",
                    ["a".repeat(64)],
                )
                .unwrap();
                reject_candidate(&conn, &project, &version, Some("unused take")).unwrap();
            }
            // Reopen: rejection persisted across a full restart.
            let conn = db::open_existing_connection(&db_path).unwrap();
            assert_eq!(
                review_state(&conn, &version).unwrap(),
                CandidateReviewState::Rejected
            );
            restore_candidate(&conn, &version).unwrap();
            drop(conn);
            let conn = db::open_existing_connection(&db_path).unwrap();
            assert_eq!(
                review_state(&conn, &version).unwrap(),
                CandidateReviewState::Active
            );
        }
    }

    // ------------------------------------------------------------------
    // Read model (Task 3): coherent review representation.
    // ------------------------------------------------------------------

    mod read_model_tests {
        use super::super::read_model::*;
        use super::super::repository::{reject_candidate, review_state};
        use super::super::CandidateReviewState;
        use crate::cinema::promotion::test_support::completed_shot_i2v_fixture;
        use crate::db;
        use crate::qa::models::{QaMediaKind, QaOverallStatus, QaRunRecord, QaRunStatus};

        #[test]
        fn captured_candidate_appears_with_provenance_and_qa_fields() {
            let fixture = completed_shot_i2v_fixture();
            let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();
            let candidates = list_shot_video_candidates(&conn, &fixture.shot_id, None).unwrap();
            assert_eq!(candidates.len(), 1);
            let candidate = &candidates[0];
            assert_eq!(candidate.shot_id, fixture.shot_id);
            assert_eq!(candidate.scene_id, fixture.scene_id);
            assert_eq!(candidate.review_state, CandidateReviewState::Active);
            assert!(!candidate.is_canonical);
            assert_eq!(candidate.qa_run_count, 0);
            assert_eq!(candidate.qa_overall_status, None);
            assert_eq!(candidate.provider_id.as_deref(), Some("fake_async_video"));
            assert_eq!(candidate.model_id.as_deref(), Some("fake-video-v1"));
            assert_eq!(
                candidate.source_asset_version_id.as_deref(),
                Some(fixture.source_version_id.as_str())
            );
            assert!(candidate.source_keyframe_is_current);
        }

        #[test]
        fn canonical_flag_resolves_from_the_exact_pin() {
            let fixture = completed_shot_i2v_fixture();
            let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();

            let none = list_shot_video_candidates(&conn, &fixture.shot_id, None).unwrap();
            assert!(!none[0].is_canonical);

            let promoted = list_shot_video_candidates(
                &conn,
                &fixture.shot_id,
                Some(&fixture.video_version_id()),
            )
            .unwrap();
            assert!(promoted[0].is_canonical);
        }

        #[test]
        fn newest_first_ordering_is_deterministic() {
            let fixture = completed_shot_i2v_fixture();
            let artifact_b = fixture.capture_extra_video_artifact("attempt-b", 2);
            let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();
            let artifact = crate::generation::repository::get_artifact_for_project(
                &conn,
                &fixture.project_id(),
                &artifact_b,
            )
            .unwrap()
            .unwrap();
            crate::assets::service::AssetService::import_media_version(
                &fixture.root,
                &fixture.video_asset_id,
                &fixture.root.join(&artifact.storage_path),
                None,
            )
            .unwrap();

            let candidates = list_shot_video_candidates(&conn, &fixture.shot_id, None).unwrap();
            assert_eq!(candidates.len(), 2);
            // Newest capture (the later version number) comes first.
            assert!(candidates[0].version_number > candidates[1].version_number);
        }

        #[test]
        fn rejected_state_and_canonical_state_stay_orthogonal() {
            let fixture = completed_shot_i2v_fixture();
            let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();
            let version = fixture.video_version_id();
            reject_candidate(&conn, &fixture.project_id(), &version, None).unwrap();

            // The gate refuses to promote a rejected candidate...
            let error = crate::cinema::promotion::promote_shot_video_candidate(
                &fixture.root,
                &fixture.shot_id,
                &fixture.artifact_id,
                None,
                None,
            )
            .unwrap_err();
            assert_eq!(error.code(), "CANDIDATE_REJECTED");
            // ...and the pin is untouched by the refused promotion.
            assert_eq!(
                resolve_canonical_video_version(&conn, &fixture.shot_id).unwrap(),
                None
            );
        }

        #[test]
        fn qa_result_from_p10_3_attaches_to_the_correct_version() {
            let fixture = completed_shot_i2v_fixture();
            let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();
            let version = fixture.video_version_id();
            insert_video_qa_run(&conn, &fixture.project_id(), &version, "fail");

            let candidates = list_shot_video_candidates(&conn, &fixture.shot_id, None).unwrap();
            assert_eq!(candidates[0].qa_overall_status.as_deref(), Some("fail"));
            assert_eq!(candidates[0].qa_run_count, 1);

            // Another fixture (different project) has no QA: results do not leak.
            let other = completed_shot_i2v_fixture();
            let conn = db::open_existing_connection(&other.root.join("project.db")).unwrap();
            let candidates = list_shot_video_candidates(&conn, &other.shot_id, None).unwrap();
            assert_eq!(candidates[0].qa_run_count, 0);
        }

        #[test]
        fn latest_qa_run_wins_over_older_ones() {
            let fixture = completed_shot_i2v_fixture();
            let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();
            let version = fixture.video_version_id();
            let asset_id: String = conn
                .query_row(
                    "SELECT asset_id FROM asset_versions WHERE id = ?1",
                    [&version],
                    |row| row.get(0),
                )
                .unwrap();
            insert_video_qa_run(&conn, &fixture.project_id(), &version, "fail");
            let rerun = QaRunRecord {
                id: "qa-rerun".to_string(),
                created_at: "2026-09-03T01:00:00Z".to_string(),
                overall_status: Some(QaOverallStatus::Pass),
                ..qa_record(&fixture.project_id(), &asset_id, &version)
            };
            crate::qa::repository::insert_run(&conn, &rerun).unwrap();

            let candidates = list_shot_video_candidates(&conn, &fixture.shot_id, None).unwrap();
            assert_eq!(candidates[0].qa_overall_status.as_deref(), Some("pass"));
            assert_eq!(candidates[0].qa_run_count, 2);
        }

        #[test]
        fn source_drift_is_detected_when_the_keyframe_pin_changes() {
            let fixture = completed_shot_i2v_fixture();
            let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();
            assert!(
                list_shot_video_candidates(&conn, &fixture.shot_id, None).unwrap()[0]
                    .source_keyframe_is_current
            );

            // Repin the Shot's keyframe to a fresh version: the candidate's
            // frozen source is now stale.
            conn.execute(
                "INSERT INTO asset_versions (id, asset_id, version_number, status, \
                 file_path, thumbnail_path, sha256, original_filename, mime_type, \
                 byte_size, created_at) \
                 VALUES ('drifted-version', (SELECT id FROM assets WHERE type = 'shot_keyframe'), \
                 99, 'canonical', 'drifted.png', '', ?, 'd.png', 'image/png', 1, 'now')",
                ["c".repeat(64)],
            )
            .unwrap();
            conn.execute(
                "UPDATE scene_shots SET keyframe_asset_version_id = 'drifted-version' \
                 WHERE id = ?1",
                [&fixture.shot_id],
            )
            .unwrap();
            assert!(
                !list_shot_video_candidates(&conn, &fixture.shot_id, None).unwrap()[0]
                    .source_keyframe_is_current
            );
        }

        #[test]
        fn unknown_shot_is_an_error_not_an_empty_list() {
            let fixture = completed_shot_i2v_fixture();
            let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();
            let error = list_shot_video_candidates(&conn, "missing-shot", None).unwrap_err();
            assert_eq!(error.code(), "SHOT_NOT_FOUND");
            let _ = review_state(&conn, "missing");
        }

        #[test]
        fn failed_attempts_without_artifacts_never_appear() {
            // Only the successful capture exists; failed captures never
            // create artifacts/versions/promotions.
            let fixture = completed_shot_i2v_fixture();
            let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();
            let candidates = list_shot_video_candidates(&conn, &fixture.shot_id, None).unwrap();
            assert_eq!(candidates.len(), 1);
        }

        #[test]
        fn canonical_resolver_returns_exact_version_or_none() {
            let fixture = completed_shot_i2v_fixture();
            let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();

            // No promotion yet: None (never "latest").
            assert_eq!(
                resolve_canonical_video_version(&conn, &fixture.shot_id).unwrap(),
                None
            );

            let promoted = crate::cinema::promotion::promote_shot_video_candidate(
                &fixture.root,
                &fixture.shot_id,
                &fixture.artifact_id,
                None,
                None,
            )
            .unwrap();
            assert_eq!(
                resolve_canonical_video_version(&conn, &fixture.shot_id).unwrap(),
                Some(promoted.asset_version_id)
            );
        }

        fn qa_record(project_id: &str, asset_id: &str, asset_version_id: &str) -> QaRunRecord {
            QaRunRecord {
                id: String::new(),
                project_id: project_id.to_string(),
                asset_id: asset_id.to_string(),
                asset_version_id: asset_version_id.to_string(),
                media_kind: QaMediaKind::Video,
                workflow_run_id: None,
                status: QaRunStatus::Succeeded,
                overall_status: None,
                adapter_id: Some("mock".to_string()),
                adapter_version: Some("1".to_string()),
                model_id: Some("mock-video-qa-v1".to_string()),
                execution_location: "local".to_string(),
                check_plan: serde_json::json!({"assetType": "video"}),
                context_snapshot: serde_json::json!({}),
                raw_response_metadata: None,
                error_code: None,
                error_message: None,
                created_at: String::new(),
                started_at: None,
                completed_at: None,
            }
        }

        fn insert_video_qa_run(
            conn: &rusqlite::Connection,
            project_id: &str,
            asset_version_id: &str,
            overall: &str,
        ) {
            let asset_id: String = conn
                .query_row(
                    "SELECT asset_id FROM asset_versions WHERE id = ?1",
                    [asset_version_id],
                    |row| row.get(0),
                )
                .unwrap();
            let record = QaRunRecord {
                id: format!("qa-{asset_version_id}"),
                created_at: "2026-09-03T00:00:00Z".to_string(),
                overall_status: Some(match overall {
                    "pass" => QaOverallStatus::Pass,
                    "needs_review" => QaOverallStatus::NeedsReview,
                    _ => QaOverallStatus::Fail,
                }),
                ..qa_record(project_id, &asset_id, asset_version_id)
            };
            crate::qa::repository::insert_run(conn, &record).unwrap();
        }

        /// Inserts one succeeded video QA run for a version; shared with
        /// the review-action tests to assert QA survival across reject.
        pub(super) fn qa_record_for_test(
            conn: &rusqlite::Connection,
            project_id: &str,
            asset_version_id: &str,
        ) {
            insert_video_qa_run(conn, project_id, asset_version_id, "pass");
        }
    }

    // ------------------------------------------------------------------
    // Review actions (Task 4): Reject / Restore at the service boundary.
    // ------------------------------------------------------------------

    mod service_tests {
        use super::super::repository::review_state;
        use super::super::service::{reject_shot_video_candidate, restore_shot_video_candidate};
        use super::super::CandidateReviewState;
        use crate::cinema::promotion::test_support::completed_shot_i2v_fixture;
        use crate::db;

        #[test]
        fn active_noncanonical_candidate_is_rejected() {
            let fixture = completed_shot_i2v_fixture();
            let version = fixture.video_version_id();
            let state = reject_shot_video_candidate(
                &fixture.root,
                &fixture.shot_id,
                &version,
                Some("unused take"),
            )
            .unwrap();
            assert_eq!(state, CandidateReviewState::Rejected);
        }

        #[test]
        fn canonical_candidate_rejection_is_refused() {
            let fixture = completed_shot_i2v_fixture();
            let promoted = crate::cinema::promotion::promote_shot_video_candidate(
                &fixture.root,
                &fixture.shot_id,
                &fixture.artifact_id,
                None,
                None,
            )
            .unwrap();
            let error = reject_shot_video_candidate(
                &fixture.root,
                &fixture.shot_id,
                &promoted.asset_version_id,
                None,
            )
            .unwrap_err();
            assert_eq!(error.code(), "CANONICAL_CANDIDATE_CANNOT_BE_REJECTED");

            // Canonical selection is unchanged — no automatic unpromotion.
            let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();
            assert_eq!(
                review_state(&conn, &promoted.asset_version_id).unwrap(),
                CandidateReviewState::Active
            );
        }

        #[test]
        fn rejecting_an_already_rejected_candidate_is_idempotent() {
            let fixture = completed_shot_i2v_fixture();
            let version = fixture.video_version_id();
            reject_shot_video_candidate(&fixture.root, &fixture.shot_id, &version, Some("first"))
                .unwrap();
            reject_shot_video_candidate(&fixture.root, &fixture.shot_id, &version, Some("second"))
                .unwrap();
            let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();
            assert_eq!(
                review_state(&conn, &version).unwrap(),
                CandidateReviewState::Rejected
            );
        }

        #[test]
        fn rejected_candidate_is_restored_to_active() {
            let fixture = completed_shot_i2v_fixture();
            let version = fixture.video_version_id();
            reject_shot_video_candidate(&fixture.root, &fixture.shot_id, &version, None).unwrap();
            let state =
                restore_shot_video_candidate(&fixture.root, &fixture.shot_id, &version).unwrap();
            assert_eq!(state, CandidateReviewState::Active);

            // Restore does not promote.
            let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();
            assert_eq!(
                super::super::read_model::resolve_canonical_video_version(&conn, &fixture.shot_id)
                    .unwrap(),
                None
            );
        }

        #[test]
        fn restoring_an_active_candidate_is_idempotent() {
            let fixture = completed_shot_i2v_fixture();
            let version = fixture.video_version_id();
            let state =
                restore_shot_video_candidate(&fixture.root, &fixture.shot_id, &version).unwrap();
            assert_eq!(state, CandidateReviewState::Active);
        }

        #[test]
        fn artifacts_and_qa_survive_rejection_and_restore() {
            let fixture = completed_shot_i2v_fixture();
            let version = fixture.video_version_id();
            let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();
            super::super::tests::read_model_tests::qa_record_for_test(
                &conn,
                &fixture.project_id(),
                &version,
            );
            drop(conn);

            reject_shot_video_candidate(&fixture.root, &fixture.shot_id, &version, None).unwrap();
            restore_shot_video_candidate(&fixture.root, &fixture.shot_id, &version).unwrap();

            let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();
            // Artifact-derived version still resolves with its file intact.
            let (file_path, byte_size): (String, i64) = conn
                .query_row(
                    "SELECT file_path, byte_size FROM asset_versions WHERE id = ?1",
                    [&version],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert!(!file_path.is_empty());
            assert!(byte_size > 0);
            // QA history intact.
            let qa_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM qa_runs WHERE asset_version_id = ?1",
                    [&version],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(qa_count, 1);
        }

        #[test]
        fn version_of_another_shot_is_not_rejectable() {
            let fixture = completed_shot_i2v_fixture();
            let other = completed_shot_i2v_fixture();
            let other_version = other.video_version_id();
            let error =
                reject_shot_video_candidate(&fixture.root, &fixture.shot_id, &other_version, None)
                    .unwrap_err();
            assert_eq!(error.code(), "ASSET_VERSION_NOT_FOUND");
        }

        #[test]
        fn unknown_version_is_not_found() {
            let fixture = completed_shot_i2v_fixture();
            let error = reject_shot_video_candidate(
                &fixture.root,
                &fixture.shot_id,
                "missing-version",
                None,
            )
            .unwrap_err();
            assert_eq!(error.code(), "ASSET_VERSION_NOT_FOUND");
        }
    }
}
