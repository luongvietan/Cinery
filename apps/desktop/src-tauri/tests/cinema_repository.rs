use cinematic_desktop_lib::cinema::model::*;
use cinematic_desktop_lib::cinema::repository;
use cinematic_desktop_lib::db;
use cinematic_desktop_lib::project::service::ProjectService;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use tempfile::{tempdir, TempDir};

fn project(name: &str) -> (TempDir, PathBuf) {
    let temp = tempdir().unwrap();
    let root = temp.path().join(name);
    ProjectService::create(&root, name).unwrap();
    (temp, root)
}

fn open_db(root: &Path) -> Connection {
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    // Integration tests open the same file the service just bootstrapped;
    // FK enforcement is session-scoped, so re-assert it here.
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    conn
}

/// Inserts an authoritative `world_scenes` row directly (repository-level
/// test; the service-level path is covered by the service/command suites).
fn insert_world_scene(conn: &Connection, project_id: &str, id: &str) {
    conn.execute(
        "INSERT INTO world_scenes (id, project_id, ordinal, title, summary, created_at, updated_at) \
         VALUES (?1, ?2, 0, 'Scene 001 - Ops Room', 'Tight station interior.', 'now', 'now')",
        params![id, project_id],
    )
    .unwrap();
}

fn shot_record(scene_id: &str, id: &str, ordering: i64) -> ShotRecord {
    ShotRecord {
        id: id.into(),
        scene_id: scene_id.into(),
        ordering,
        duration_seconds: 4.0,
        keyframe_asset_version_id: None,
        generated_video_asset_version_id: None,
        intent: "Establish the ops room".into(),
        action: Some("Mara scans the console".into()),
        camera: Some("wide".into()),
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

#[test]
fn creates_shots_with_ordering_uniqueness_and_project_scoping() {
    let (_temp, root) = project("Red Door");
    let conn = open_db(&root);
    let project_id: String = conn
        .query_row("SELECT id FROM projects LIMIT 1", [], |row| row.get(0))
        .unwrap();

    insert_world_scene(&conn, &project_id, "scene-1");

    // Shots cannot be attached to a scene from another project.
    assert!(repository::ensure_scene_in_project(&conn, "other-project", "scene-1").is_err());
    assert!(repository::ensure_scene_in_project(&conn, &project_id, "scene-1").is_ok());

    repository::create_shot(&conn, &shot_record("scene-1", "shot-1", 0)).unwrap();
    repository::create_shot(&conn, &shot_record("scene-1", "shot-2", 1)).unwrap();

    // Duplicate (scene_id, ordering) is rejected.
    assert!(repository::create_shot(&conn, &shot_record("scene-1", "shot-3", 0)).is_err());

    // FK: shots must reference a real authoritative scene.
    assert!(repository::create_shot(&conn, &shot_record("no-such-scene", "shot-4", 0)).is_err());

    let shots = repository::list_shots(&conn, "scene-1").unwrap();
    assert_eq!(shots.len(), 2);
    assert_eq!(shots[0].ordering, 0);
    assert_eq!(shots[1].ordering, 1);
    assert_eq!(shots[0].scene_id, "scene-1");
}

#[test]
fn compilations_round_trip_with_scene_foreign_key() {
    let (_temp, root) = project("Red Door");
    let conn = open_db(&root);
    let project_id: String = conn
        .query_row("SELECT id FROM projects LIMIT 1", [], |row| row.get(0))
        .unwrap();

    // FK: compilation must reference a real authoritative scene.
    let orphan = CinemaCompilation {
        id: "c-1".into(),
        project_id: project_id.clone(),
        scene_id: "no-such-scene".into(),
        input_json: "{}".into(),
        compilation_json: "{}".into(),
        export_path: "prompts/cinema/c-1.json".into(),
        export_sha256: "a".repeat(64),
        created_at: "now".into(),
    };
    assert!(repository::insert_compilation(&conn, &orphan).is_err());

    insert_world_scene(&conn, &project_id, "scene-1");
    let record = CinemaCompilation {
        scene_id: "scene-1".into(),
        ..orphan
    };
    repository::insert_compilation(&conn, &record).unwrap();

    let fetched = repository::get_compilation(&conn, "c-1").unwrap();
    assert_eq!(fetched.export_path, "prompts/cinema/c-1.json");
    assert_eq!(fetched.export_sha256, "a".repeat(64));

    assert!(repository::get_compilation(&conn, "missing").is_err());

    let listed = repository::list_compilations(&conn, "scene-1").unwrap();
    assert_eq!(listed.len(), 1);
}

#[test]
fn reorder_shots_is_validated_and_contiguous() {
    let (_temp, root) = project("Red Door");
    let mut conn = open_db(&root);
    let project_id: String = conn
        .query_row("SELECT id FROM projects LIMIT 1", [], |row| row.get(0))
        .unwrap();

    insert_world_scene(&conn, &project_id, "scene-1");
    repository::create_shot(&conn, &shot_record("scene-1", "shot-1", 0)).unwrap();
    repository::create_shot(&conn, &shot_record("scene-1", "shot-2", 1)).unwrap();
    repository::create_shot(&conn, &shot_record("scene-1", "shot-3", 2)).unwrap();

    // Incomplete set is rejected without side effects.
    assert!(repository::reorder_shots(
        &mut conn,
        &project_id,
        "scene-1",
        &["shot-2".into(), "shot-1".into()]
    )
    .is_err());
    // Foreign/duplicate ids are rejected.
    assert!(repository::reorder_shots(
        &mut conn,
        &project_id,
        "scene-1",
        &["shot-1".into(), "shot-1".into(), "shot-2".into()]
    )
    .is_err());

    let reordered = repository::reorder_shots(
        &mut conn,
        &project_id,
        "scene-1",
        &["shot-3".into(), "shot-1".into(), "shot-2".into()],
    )
    .unwrap();
    let orderings: Vec<i64> = reordered.iter().map(|shot| shot.ordering).collect();
    assert_eq!(orderings, vec![0, 1, 2]);
    assert_eq!(reordered[0].id, "shot-3");
}
