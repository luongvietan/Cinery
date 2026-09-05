//! Command-boundary tests for the sequence-first flow. Scenes are created on
//! the authoritative `world_scenes` aggregate; the sequence flow persists one
//! explicit workflow record per scene. Every mutation goes through public
//! Tauri command functions.

use cinematic_desktop_lib::assets::service::AssetService;
use cinematic_desktop_lib::canon::model::CanonEntityType;
use cinematic_desktop_lib::canon::service::CanonService;
use cinematic_desktop_lib::cinema::commands::*;
use cinematic_desktop_lib::cinema::sequence_flow::{SequenceBriefInput, SequenceFlowService, SequenceStage};
use cinematic_desktop_lib::db;
use cinematic_desktop_lib::error::{AppCommandError, AppError};
use cinematic_desktop_lib::project::service::ProjectService;
use cinematic_desktop_lib::scenes::commands as scene_commands;
use cinematic_desktop_lib::scenes::model::Scene;
use cinematic_desktop_lib::worlds::service::WorldService;
use rusqlite::params;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn mp4_bytes(seed: u8) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x00, 0x00, 0x18];
    bytes.extend_from_slice(b"ftypmp42");
    bytes.extend_from_slice(&[0x00, 0x00, 0x00, seed]);
    bytes.extend_from_slice(b"mp42isom");
    bytes
}

fn image_file(root: &Path, name: &str, pixel: [u8; 4]) -> PathBuf {
    let path = root.join(name);
    let image: image::RgbaImage = image::ImageBuffer::from_pixel(8, 8, image::Rgba(pixel));
    image.save(&path).unwrap();
    path
}

struct Fixture {
    _temp: tempfile::TempDir,
    root: String,
}

fn fixture() -> Fixture {
    let temp = tempdir().unwrap();
    let path = temp.path().join("sequence-flow");
    ProjectService::create(&path, "Sequence Flow").unwrap();
    Fixture {
        _temp: temp,
        root: path.to_string_lossy().to_string(),
    }
}

fn brief(intent: &str) -> SequenceBriefInput {
    SequenceBriefInput {
        intent: intent.to_string(),
        energy: "elevated".to_string(),
        target_duration_seconds: Some(15.0),
        credit_cap: 800,
    }
}

fn create_scene(f: &Fixture) -> Scene {
    scene_commands::create_world_scene(
        f.root.clone(),
        "Scene 001".into(),
        "Ops room stand-off".into(),
    )
    .unwrap()
}

fn assert_err_contains<T: std::fmt::Debug>(result: Result<T, AppCommandError>, needle: &str) {
    let error = result.unwrap_err();
    assert!(
        error.message.contains(needle),
        "expected an error containing {needle:?}, got {:?}",
        error.message
    );
}

/// Assembles the scene references the readiness gate requires: a World whose
/// plate has a canonical version, one cast character with a canonical look,
/// and one shot.
fn complete_references(f: &Fixture, scene_id: &str) {
    let root = Path::new(&f.root);
    let location =
        CanonService::create_entity(root, CanonEntityType::Location, "The Station").unwrap();
    let world = WorldService::create_world(root, &location.id).unwrap();
    let plate_source = image_file(root, "world-plate.png", [10, 20, 30, 255]);
    let plate_version =
        AssetService::import_asset_version(root, &world.world_plate_asset_id, &plate_source, None)
            .unwrap();
    AssetService::promote_asset_version(root, &plate_version.id).unwrap();
    scene_commands::assign_scene_world(f.root.clone(), scene_id.to_string(), world.id).unwrap();

    let character =
        CanonService::create_entity(root, CanonEntityType::Character, "Mara Keene").unwrap();
    let look_asset =
        AssetService::create_asset(root, "outfit", "Mara Look", Some(character.id.clone()))
            .unwrap();
    let look_source = image_file(root, "look.png", [4, 5, 6, 255]);
    let look_version =
        AssetService::import_asset_version(root, &look_asset.id, &look_source, None).unwrap();
    AssetService::promote_asset_version(root, &look_version.id).unwrap();
    scene_commands::add_world_scene_character(
        f.root.clone(),
        scene_id.to_string(),
        character.id,
        look_version.id,
        None,
        None,
    )
    .unwrap();

    create_shot(
        f.root.clone(),
        scene_id.to_string(),
        None,
        4.0,
        "Establish".into(),
        None,
        None,
    )
    .unwrap();
}

/// Pins a canonical generated-video version onto the scene's shot. Production
/// promotes a captured candidate; the resulting exact pin is the same
/// reference `resolve_canonical_shot_video` resolves for extensions.
fn promote_fixture_candidate(f: &Fixture, scene: &Scene) -> (String, String) {
    let root = Path::new(&f.root);
    let shot = create_shot(
        f.root.clone(),
        scene.id.clone(),
        None,
        4.0,
        "Establish".into(),
        None,
        None,
    )
    .unwrap();
    let video_asset =
        AssetService::create_asset(root, "video", "Scene 001 video", Some(scene.id.clone()))
            .unwrap();
    let source = root.join("candidate.mp4");
    std::fs::write(&source, mp4_bytes(7)).unwrap();
    let version = AssetService::import_media_version(root, &video_asset.id, &source, None).unwrap();
    AssetService::promote_asset_version(root, &version.id).unwrap();
    set_shot_video(f.root.clone(), shot.id.clone(), Some(version.id.clone())).unwrap();
    (shot.id, version.id)
}

#[test]
fn extension_requires_a_canonical_video_and_explicit_direction() {
    let fixture = fixture();
    let scene = create_scene(&fixture);
    assert_err_contains(
        prepare_sequence_extension(fixture.root.clone(), scene.id.clone(), "prequel".into()),
        "canonical video",
    );
    let (shot_id, version_id) = promote_fixture_candidate(&fixture, &scene);
    let prepared =
        prepare_sequence_extension(fixture.root.clone(), scene.id.clone(), "sequel".into())
            .unwrap();
    assert_eq!(prepared.direction.as_str(), "sequel");
    assert_eq!(prepared.shot_id, shot_id);
    assert_eq!(prepared.canonical_video_asset_version_id, version_id);
    assert_eq!(prepared.scene_id, scene.id);
    // Only the two deliberate directions are accepted.
    assert_err_contains(
        prepare_sequence_extension(fixture.root, scene.id, "continue".into()),
        "direction",
    );
}

#[test]
fn stage_transitions_reject_skips_and_conflicts_without_losing_state() {
    let f = fixture();
    let scene = create_scene(&f);
    let flow =
        update_sequence_brief(f.root.clone(), scene.id.clone(), brief("Tay notices the door"))
            .unwrap();
    assert_eq!(flow.stage.as_str(), "brief_locked");
    assert_eq!(flow.brief.intent, "Tay notices the door");
    assert_eq!(flow.brief.energy.as_str(), "elevated");
    assert_eq!(flow.brief.credit_cap, 800);

    // Draft -> Prompt approved is not an adjacent transition: the approval
    // command's compare-and-set refuses the row that is still at
    // brief_locked.
    let error =
        approve_sequence_preflight(f.root.clone(), scene.id.clone(), None).unwrap_err();
    assert_eq!(error.code, "SEQUENCE_FLOW_STAGE_CONFLICT");
    let error = begin_sequence_review(f.root.clone(), scene.id.clone()).unwrap_err();
    assert_eq!(error.code, "SEQUENCE_FLOW_STAGE_CONFLICT");

    let unchanged = get_sequence_flow(f.root.clone(), scene.id.clone()).unwrap();
    assert_eq!(unchanged.stage.as_str(), "brief_locked");
    assert_eq!(unchanged.brief.intent, "Tay notices the door");
    assert_eq!(unchanged.updated_at, flow.updated_at);

    // The compare-and-set helper itself refuses skips and backwards moves.
    let conn =
        db::open_existing_connection(&Path::new(&f.root).join("project.db")).unwrap();
    let error = SequenceFlowService::transition(
        &conn,
        &scene.id,
        SequenceStage::Draft,
        SequenceStage::PromptApproved,
    )
    .unwrap_err();
    assert!(matches!(error, AppError::WorkflowInvalidTransition(_)));
    let error = SequenceFlowService::transition(
        &conn,
        &scene.id,
        SequenceStage::ReadyForEdit,
        SequenceStage::Draft,
    )
    .unwrap_err();
    assert!(matches!(error, AppError::WorkflowInvalidTransition(_)));
    // The adjacent, expected-stage move is the only allowed change.
    SequenceFlowService::transition(
        &conn,
        &scene.id,
        SequenceStage::BriefLocked,
        SequenceStage::ReferencesReady,
    )
    .unwrap();
    drop(conn);
    let advanced = get_sequence_flow(f.root, scene.id).unwrap();
    assert_eq!(advanced.stage.as_str(), "references_ready");
}

#[test]
fn failed_generation_transition_preserves_the_locked_brief() {
    let f = fixture();
    let scene = create_scene(&f);
    let locked = update_sequence_brief(
        f.root.clone(),
        scene.id.clone(),
        brief("A tired man hears a bell"),
    )
    .unwrap();

    // Another writer moves the flow first; the stale approval attempt must
    // lose the compare-and-set without touching the row.
    let conn =
        db::open_existing_connection(&Path::new(&f.root).join("project.db")).unwrap();
    conn.execute(
        "UPDATE sequence_flows SET stage = 'generating' WHERE scene_id = ?1",
        params![scene.id],
    )
    .unwrap();
    drop(conn);

    let error =
        approve_sequence_preflight(f.root.clone(), scene.id.clone(), None).unwrap_err();
    assert_eq!(error.code, "SEQUENCE_FLOW_STAGE_CONFLICT");

    let flow = get_sequence_flow(f.root.clone(), scene.id.clone()).unwrap();
    assert_eq!(flow.stage.as_str(), "generating");
    assert_eq!(flow.brief.intent, "A tired man hears a bell");
    assert_eq!(flow.brief.energy.as_str(), "elevated");
    assert_eq!(flow.brief.credit_cap, 800);
    assert_eq!(flow.created_at, locked.created_at);
    assert_eq!(flow.updated_at, locked.updated_at);
}

#[test]
fn mark_references_ready_returns_blockers_without_mutating_state() {
    let f = fixture();
    let scene = create_scene(&f);
    update_sequence_brief(f.root.clone(), scene.id.clone(), brief("Tay counts the exits"))
        .unwrap();

    let blocked = mark_sequence_references_ready(f.root.clone(), scene.id.clone()).unwrap();
    assert!(blocked.flow.is_none());
    let codes: Vec<&str> = blocked.blockers.iter().map(|b| b.code.as_str()).collect();
    assert!(codes.contains(&"world_reference_missing"));
    assert!(codes.contains(&"no_cast"));
    assert!(codes.contains(&"no_shots"));

    // The blocked report never mutated the flow.
    let flow = get_sequence_flow(f.root.clone(), scene.id.clone()).unwrap();
    assert_eq!(flow.stage.as_str(), "brief_locked");

    complete_references(&f, &scene.id);

    let ready = mark_sequence_references_ready(f.root.clone(), scene.id.clone()).unwrap();
    assert!(ready.blockers.is_empty());
    assert_eq!(ready.flow.unwrap().stage.as_str(), "references_ready");

    // Pre-flight approval completes the adjacent transition.
    let approved = approve_sequence_preflight(f.root.clone(), scene.id.clone(), None).unwrap();
    assert_eq!(approved.stage.as_str(), "prompt_approved");
    assert_eq!(approved.approved_compilation_id, None);

    // The brief is locked once the flow has moved on.
    let error =
        update_sequence_brief(f.root.clone(), scene.id.clone(), brief("Rewritten")).unwrap_err();
    assert_eq!(error.code, "WORKFLOW_INVALID_TRANSITION");
    let flow = get_sequence_flow(f.root, scene.id).unwrap();
    assert_eq!(flow.brief.intent, "Tay counts the exits");
}

/// Walks a flow from `generating` to `in_review` so canonical-take tests can
/// start from the review stage.
fn begin_review(f: &Fixture, scene_id: &str) {
    let conn =
        db::open_existing_connection(&Path::new(&f.root).join("project.db")).unwrap();
    conn.execute(
        "UPDATE sequence_flows SET stage = 'generating' WHERE scene_id = ?1",
        params![scene_id],
    )
    .unwrap();
    drop(conn);
    begin_sequence_review(f.root.clone(), scene_id.to_string()).unwrap();
}

#[test]
fn canonical_take_requires_a_pinned_video_and_the_review_stage() {
    let f = fixture();
    let scene = create_scene(&f);
    update_sequence_brief(f.root.clone(), scene.id.clone(), brief("Tay picks the take"))
        .unwrap();

    // A shot without a pinned canonical video cannot be selected, at any
    // stage; the flow itself does not even exist at brief_locked.
    complete_references(&f, &scene.id);
    let shot = create_shot(
        f.root.clone(),
        scene.id.clone(),
        None,
        4.0,
        "Establish".into(),
        None,
        None,
    )
    .unwrap();
    assert_err_contains(
        mark_sequence_canonical_take(f.root.clone(), scene.id.clone(), shot.id.clone()),
        "canonical video",
    );

    // Promote a candidate for the shot, move to in_review, then select.
    let (pinned_shot, version_id) = promote_fixture_candidate(&f, &scene);
    begin_review(&f, &scene.id);
    let flow = mark_sequence_canonical_take(f.root.clone(), scene.id.clone(), pinned_shot.clone())
        .unwrap();
    assert_eq!(flow.stage.as_str(), "canonical_selected");
    assert_eq!(flow.canonical_shot_id.as_deref(), Some(pinned_shot.as_str()));

    // The recorded take is the exact pinned version.
    let resolved = resolve_canonical_shot_video(f.root.clone(), pinned_shot).unwrap();
    assert_eq!(resolved.as_deref(), Some(version_id.as_str()));
}

#[test]
fn begin_review_moves_generating_to_in_review() {
    let f = fixture();
    let scene = create_scene(&f);
    update_sequence_brief(f.root.clone(), scene.id.clone(), brief("Tay hears the bell")).unwrap();
    let conn =
        db::open_existing_connection(&Path::new(&f.root).join("project.db")).unwrap();
    conn.execute(
        "UPDATE sequence_flows SET stage = 'generating' WHERE scene_id = ?1",
        params![scene.id],
    )
    .unwrap();
    drop(conn);

    let flow = begin_sequence_review(f.root, scene.id).unwrap();
    assert_eq!(flow.stage.as_str(), "in_review");
}

#[test]
fn sequence_briefs_are_validated_before_any_write() {
    let f = fixture();
    let scene = create_scene(&f);

    assert_eq!(
        update_sequence_brief(f.root.clone(), scene.id.clone(), brief("   "))
            .unwrap_err()
            .code,
        "INVALID_SEQUENCE_BRIEF"
    );
    let mut bad_energy = brief("Beat");
    bad_energy.energy = "whimsical".to_string();
    assert_eq!(
        update_sequence_brief(f.root.clone(), scene.id.clone(), bad_energy)
            .unwrap_err()
            .code,
        "INVALID_SEQUENCE_BRIEF"
    );
    let mut zero_duration = brief("Beat");
    zero_duration.target_duration_seconds = Some(0.0);
    assert_eq!(
        update_sequence_brief(f.root.clone(), scene.id.clone(), zero_duration)
            .unwrap_err()
            .code,
        "INVALID_SEQUENCE_BRIEF"
    );
    let mut long_duration = brief("Beat");
    long_duration.target_duration_seconds = Some(121.0);
    assert_eq!(
        update_sequence_brief(f.root.clone(), scene.id.clone(), long_duration)
            .unwrap_err()
            .code,
        "INVALID_SEQUENCE_BRIEF"
    );
    let mut negative_cap = brief("Beat");
    negative_cap.credit_cap = -1;
    assert_eq!(
        update_sequence_brief(f.root.clone(), scene.id.clone(), negative_cap)
            .unwrap_err()
            .code,
        "INVALID_SEQUENCE_BRIEF"
    );

    // Nothing was written by the rejected briefs.
    assert_eq!(
        get_sequence_flow(f.root.clone(), scene.id.clone())
            .unwrap_err()
            .code,
        "SEQUENCE_FLOW_NOT_FOUND"
    );
    // Scenes of other projects are reported as missing, never leaked.
    assert_eq!(
        get_sequence_flow(f.root, "01ARZ3NDEKTSV4RRFFQ69G5FAV".into())
            .unwrap_err()
            .code,
        "SCENE_NOT_FOUND"
    );
}

/// Task 7 acceptance coverage: the full happy path in one continuous flow —
/// brief -> references ready -> prompt approved -> generating -> in review ->
/// canonical selected -> extension prepared — exercised end to end through
/// the public command surface rather than in isolated per-transition tests.
/// The single stage this command set does not expose (prompt_approved ->
/// generating, which the workflow runtime owns) is advanced the same way the
/// other tests above do: a direct row update simulating that the provider
/// run has started, immediately followed by the explicit `begin_sequence_review`
/// command that only this test suite is responsible for covering end to end.
#[test]
fn full_sequence_flow_reaches_a_prepared_extension_in_one_continuous_journey() {
    let f = fixture();
    let scene = create_scene(&f);

    // 1. Lock the director brief.
    let locked = update_sequence_brief(
        f.root.clone(),
        scene.id.clone(),
        brief("Tay notices the door"),
    )
    .unwrap();
    assert_eq!(locked.stage.as_str(), "brief_locked");

    // 2. Attach the required references (world plate, cast look, one shot),
    // then explicitly mark them ready.
    complete_references(&f, &scene.id);
    let references_ready =
        mark_sequence_references_ready(f.root.clone(), scene.id.clone()).unwrap();
    assert!(references_ready.blockers.is_empty());
    assert_eq!(
        references_ready.flow.unwrap().stage.as_str(),
        "references_ready"
    );

    // 3. Approve the generation preflight.
    let approved = approve_sequence_preflight(f.root.clone(), scene.id.clone(), None).unwrap();
    assert_eq!(approved.stage.as_str(), "prompt_approved");
    // The locked brief survives every explicit transition unchanged.
    assert_eq!(approved.brief.intent, "Tay notices the door");

    // 4. The provider run starts (owned by the workflow runtime, outside this
    // command set) and completes; the flow moves into review.
    let conn = db::open_existing_connection(&Path::new(&f.root).join("project.db")).unwrap();
    conn.execute(
        "UPDATE sequence_flows SET stage = 'generating' WHERE scene_id = ?1",
        params![scene.id],
    )
    .unwrap();
    drop(conn);
    let in_review = begin_sequence_review(f.root.clone(), scene.id.clone()).unwrap();
    assert_eq!(in_review.stage.as_str(), "in_review");

    // 5. Promote one of the generated candidates as the shot's canonical take.
    let (shot_id, version_id) = promote_fixture_candidate(&f, &scene);
    let canonical =
        mark_sequence_canonical_take(f.root.clone(), scene.id.clone(), shot_id.clone()).unwrap();
    assert_eq!(canonical.stage.as_str(), "canonical_selected");
    assert_eq!(canonical.canonical_shot_id.as_deref(), Some(shot_id.as_str()));

    // 6. Prepare — not execute — a sequel extension from the exact canonical
    // pin. No credits are spent and no provider work is enqueued by this call.
    let prepared =
        prepare_sequence_extension(f.root.clone(), scene.id.clone(), "sequel".into()).unwrap();
    assert_eq!(prepared.direction.as_str(), "sequel");
    assert_eq!(prepared.scene_id, scene.id);
    assert_eq!(prepared.shot_id, shot_id);
    assert_eq!(prepared.canonical_video_asset_version_id, version_id);

    // The brief locked in step 1 is still exactly what was written.
    let final_flow = get_sequence_flow(f.root, scene.id).unwrap();
    assert_eq!(final_flow.brief.intent, "Tay notices the door");
    assert_eq!(final_flow.stage.as_str(), "canonical_selected");
}
