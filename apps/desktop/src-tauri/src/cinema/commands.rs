use crate::cinema::model::{CinemaCompilation, ShotRecord};
use crate::cinema::service::CinemaService;
use crate::error::AppCommandError;
use crate::project::service::validate_root_path;
use std::path::Path;

fn root_path(project_root_path: &str) -> Result<&Path, AppCommandError> {
    validate_root_path(project_root_path)?;
    Ok(Path::new(project_root_path))
}

/// Creates a shot on the authoritative Scene (`world_scenes`); when
/// `ordering` is omitted the shot is appended after the existing shots.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn create_shot(
    project_root_path: String,
    scene_id: String,
    ordering: Option<i64>,
    duration_seconds: f64,
    intent: String,
    action: Option<String>,
    camera: Option<String>,
) -> Result<ShotRecord, AppCommandError> {
    CinemaService::create_shot(
        root_path(&project_root_path)?,
        &scene_id,
        ordering,
        duration_seconds,
        &intent,
        action,
        camera,
    )
    .map_err(AppCommandError::from)
}

#[tauri::command]
pub fn list_shots(
    project_root_path: String,
    scene_id: String,
) -> Result<Vec<ShotRecord>, AppCommandError> {
    CinemaService::list_shots(root_path(&project_root_path)?, &scene_id)
        .map_err(AppCommandError::from)
}

#[tauri::command]
pub fn compile_cinema(
    project_root_path: String,
    scene_id: String,
    total_duration_seconds: f64,
    shot_count: Option<usize>,
) -> Result<CinemaCompilation, AppCommandError> {
    CinemaService::compile_scene(
        root_path(&project_root_path)?,
        crate::cinema::model::CinemaCompileInput {
            scene_id,
            total_duration_seconds,
            shot_count,
        },
    )
    .map_err(AppCommandError::from)
}

#[tauri::command]
pub fn get_cinema_compilation(
    project_root_path: String,
    compilation_id: String,
) -> Result<CinemaCompilation, AppCommandError> {
    CinemaService::get_compilation(root_path(&project_root_path)?, &compilation_id)
        .map_err(AppCommandError::from)
}

#[tauri::command]
pub fn list_cinema_compilations(
    project_root_path: String,
    scene_id: String,
) -> Result<Vec<CinemaCompilation>, AppCommandError> {
    CinemaService::list_compilations(root_path(&project_root_path)?, &scene_id)
        .map_err(AppCommandError::from)
}

#[tauri::command]
pub fn update_shot(
    project_root_path: String,
    shot_id: String,
    duration_seconds: Option<f64>,
    intent: Option<String>,
    action: Option<String>,
    camera: Option<String>,
) -> Result<ShotRecord, AppCommandError> {
    CinemaService::update_shot(
        root_path(&project_root_path)?,
        &crate::cinema::model::ShotUpdate {
            shot_id,
            duration_seconds,
            intent,
            action,
            camera,
        },
    )
    .map_err(AppCommandError::from)
}

#[tauri::command]
pub fn delete_shot(
    project_root_path: String,
    scene_id: String,
    shot_id: String,
) -> Result<(), AppCommandError> {
    CinemaService::delete_shot(root_path(&project_root_path)?, &scene_id, &shot_id)
        .map_err(AppCommandError::from)
}

#[tauri::command]
pub fn reorder_shots(
    project_root_path: String,
    scene_id: String,
    ordered_shot_ids: Vec<String>,
) -> Result<Vec<ShotRecord>, AppCommandError> {
    CinemaService::reorder_shots(root_path(&project_root_path)?, &scene_id, &ordered_shot_ids)
        .map_err(AppCommandError::from)
}

#[tauri::command]
pub fn set_shot_keyframe(
    project_root_path: String,
    shot_id: String,
    keyframe_asset_version_id: Option<String>,
) -> Result<(), AppCommandError> {
    CinemaService::set_shot_keyframe(
        root_path(&project_root_path)?,
        &shot_id,
        keyframe_asset_version_id.as_deref(),
    )
    .map_err(AppCommandError::from)
}

/// Pins (or clears) the shot's exact generated-video AssetVersion. The
/// reference never drifts when newer video versions are promoted.
#[tauri::command]
pub fn set_shot_video(
    project_root_path: String,
    shot_id: String,
    video_asset_version_id: Option<String>,
) -> Result<(), AppCommandError> {
    CinemaService::set_shot_video(
        root_path(&project_root_path)?,
        &shot_id,
        video_asset_version_id.as_deref(),
    )
    .map_err(AppCommandError::from)
}

/// Display-only projection of the Shot's exact pinned keyframe — the frozen
/// source an image-to-video run will use.
#[tauri::command]
pub fn get_shot_image_to_video_source(
    project_root_path: String,
    shot_id: String,
) -> Result<crate::cinema::model::ShotImageToVideoSource, AppCommandError> {
    CinemaService::get_shot_image_to_video_source(root_path(&project_root_path)?, &shot_id)
        .map_err(AppCommandError::from)
}

/// Promotes one exact captured `shot.image_to_video` candidate onto the
/// Shot's video pin under explicit human review. Conflict-safe: a stale
/// expected pin returns `PROMOTION_CONFLICT` without overwriting the winner.
/// An exceptional candidate (QA failed/needs-review, stale frozen inputs)
/// requires a non-empty `override_reason`, audited as a QA override.
#[tauri::command]
pub fn promote_shot_video_candidate(
    project_root_path: String,
    shot_id: String,
    artifact_id: String,
    expected_current_video_asset_version_id: Option<String>,
    override_reason: Option<String>,
) -> Result<crate::cinema::promotion::ShotVideoPromotionResult, AppCommandError> {
    crate::cinema::promotion::promote_shot_video_candidate(
        root_path(&project_root_path)?,
        &shot_id,
        &artifact_id,
        expected_current_video_asset_version_id.as_deref(),
        override_reason.as_deref(),
    )
    .map_err(AppCommandError::from)
}

#[tauri::command]
pub fn get_scene_readiness(
    project_root_path: String,
    scene_id: String,
) -> Result<crate::cinema::service::CinemaReadiness, AppCommandError> {
    CinemaService::scene_readiness(root_path(&project_root_path)?, &scene_id)
        .map_err(AppCommandError::from)
}

/// Lists every successful video candidate of a Shot for the review UI,
/// newest first, with QA summary, review state, canonical state, and
/// provenance resolved server-side (P10.4 read model).
#[tauri::command]
pub fn list_shot_video_candidates(
    project_root_path: String,
    shot_id: String,
) -> Result<Vec<crate::cinema::review::read_model::ShotVideoCandidate>, AppCommandError> {
    let root = root_path(&project_root_path)?;
    let conn = crate::db::open_existing_connection(&root.join("project.db"))
        .map_err(AppCommandError::from)?;
    let canonical =
        crate::cinema::review::read_model::resolve_canonical_video_version(&conn, &shot_id)
            .map_err(AppCommandError::from)?;
    crate::cinema::review::read_model::list_shot_video_candidates(
        &conn,
        &shot_id,
        canonical.as_deref(),
    )
    .map_err(AppCommandError::from)
}

/// Resolves the Shot's canonical video version: the exact promoted pin, or
/// None. Never falls back to the latest generation (P10.4 §5).
#[tauri::command]
pub fn resolve_canonical_shot_video(
    project_root_path: String,
    shot_id: String,
) -> Result<Option<String>, AppCommandError> {
    let root = root_path(&project_root_path)?;
    let conn = crate::db::open_existing_connection(&root.join("project.db"))
        .map_err(AppCommandError::from)?;
    crate::cinema::review::read_model::resolve_canonical_video_version(&conn, &shot_id)
        .map_err(AppCommandError::from)
}

/// Rejects one shot video candidate (review state). The current canonical
/// video cannot be rejected; artifacts and QA records remain intact.
#[tauri::command]
pub fn reject_shot_video_candidate(
    project_root_path: String,
    shot_id: String,
    asset_version_id: String,
    reason: Option<String>,
) -> Result<crate::cinema::review::CandidateReviewState, AppCommandError> {
    crate::cinema::review::service::reject_shot_video_candidate(
        root_path(&project_root_path)?,
        &shot_id,
        &asset_version_id,
        reason.as_deref(),
    )
    .map_err(AppCommandError::from)
}

/// Restores a rejected shot video candidate to Active. Never promotes.
#[tauri::command]
pub fn restore_shot_video_candidate(
    project_root_path: String,
    shot_id: String,
    asset_version_id: String,
) -> Result<crate::cinema::review::CandidateReviewState, AppCommandError> {
    crate::cinema::review::service::restore_shot_video_candidate(
        root_path(&project_root_path)?,
        &shot_id,
        &asset_version_id,
    )
    .map_err(AppCommandError::from)
}

// ---------------------------------------------------------------------------
// Sequence-first flow (Joey): explicit, guarded stage transitions. Every
// mutation below is a deliberate user action; the flow state is persisted
// per scene and all stage changes are compare-and-set guarded.
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_sequence_flow(
    project_root_path: String,
    scene_id: String,
) -> Result<crate::cinema::sequence_flow::SequenceFlowRecord, AppCommandError> {
    crate::cinema::sequence_flow::SequenceFlowService::get_flow(
        root_path(&project_root_path)?,
        &scene_id,
    )
    .map_err(AppCommandError::from)
}

#[tauri::command]
pub fn update_sequence_brief(
    project_root_path: String,
    scene_id: String,
    brief: crate::cinema::sequence_flow::SequenceBriefInput,
) -> Result<crate::cinema::sequence_flow::SequenceFlowRecord, AppCommandError> {
    crate::cinema::sequence_flow::SequenceFlowService::update_brief(
        root_path(&project_root_path)?,
        &scene_id,
        &brief,
    )
    .map_err(AppCommandError::from)
}

#[tauri::command]
pub fn mark_sequence_references_ready(
    project_root_path: String,
    scene_id: String,
) -> Result<crate::cinema::sequence_flow::ReferencesReadyReport, AppCommandError> {
    crate::cinema::sequence_flow::SequenceFlowService::mark_references_ready(
        root_path(&project_root_path)?,
        &scene_id,
    )
    .map_err(AppCommandError::from)
}

#[tauri::command]
pub fn approve_sequence_preflight(
    project_root_path: String,
    scene_id: String,
    approved_compilation_id: Option<String>,
) -> Result<crate::cinema::sequence_flow::SequenceFlowRecord, AppCommandError> {
    crate::cinema::sequence_flow::SequenceFlowService::approve_preflight(
        root_path(&project_root_path)?,
        &scene_id,
        approved_compilation_id,
    )
    .map_err(AppCommandError::from)
}

#[tauri::command]
pub fn begin_sequence_review(
    project_root_path: String,
    scene_id: String,
) -> Result<crate::cinema::sequence_flow::SequenceFlowRecord, AppCommandError> {
    crate::cinema::sequence_flow::SequenceFlowService::begin_review(
        root_path(&project_root_path)?,
        &scene_id,
    )
    .map_err(AppCommandError::from)
}

#[tauri::command]
pub fn mark_sequence_canonical_take(
    project_root_path: String,
    scene_id: String,
    shot_id: String,
) -> Result<crate::cinema::sequence_flow::SequenceFlowRecord, AppCommandError> {
    crate::cinema::sequence_flow::SequenceFlowService::mark_canonical_take(
        root_path(&project_root_path)?,
        &scene_id,
        &shot_id,
    )
    .map_err(AppCommandError::from)
}

#[tauri::command]
pub fn prepare_sequence_extension(
    project_root_path: String,
    scene_id: String,
    direction: String,
) -> Result<crate::cinema::sequence_flow::ExtensionRequestRecord, AppCommandError> {
    crate::cinema::sequence_flow::SequenceFlowService::prepare_extension(
        root_path(&project_root_path)?,
        &scene_id,
        &direction,
    )
    .map_err(AppCommandError::from)
}

