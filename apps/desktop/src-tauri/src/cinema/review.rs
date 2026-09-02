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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// explicitly acknowledge: failed/needs-review QA or stale inputs.
    pub fn is_exceptional(&self) -> bool {
        self.qa_overall_status.as_deref() != Some("pass")
            || !self.source_shot_version_current
            || !self.source_keyframe_is_current_pin
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
}
