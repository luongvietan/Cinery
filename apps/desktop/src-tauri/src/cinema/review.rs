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
}
