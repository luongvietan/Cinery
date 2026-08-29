//! Shot-domain CRUD repository tests on the authoritative `scene_shots`
//! table: field updates, keyframe assignment, transactional reorder, delete,
//! and persistence after reopen. Scene/cast/prop CRUD is covered by the
//! `scenes` module suites; these tests seed a `world_scenes` row directly.

use cinematic_desktop_lib::cinema::model::*;
use cinematic_desktop_lib::cinema::repository;
use cinematic_desktop_lib::db;
use cinematic_desktop_lib::project::service::ProjectService;
use rusqlite::{params, Connection};
use std::path::PathBuf;
use tempfile::{tempdir, TempDir};

fn project(name: &str) -> (TempDir, PathBuf) {
    let temp = tempdir().unwrap();
    let root = temp.path().join(name);
    ProjectService::create(&root, name).unwrap();
    (temp, root)
}

fn open_db(root: &PathBuf) -> Connection {
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    conn
}

/// Seeds minimal, FK-valid asset version rows so keyframe pins satisfy the
/// schema's foreign keys.
fn seed_versions(conn: &Connection, version_ids: &[&str]) {
    let project_id: String = conn
        .query_row("SELECT id FROM projects", [], |row| row.get(0))
        .unwrap();
    for (index, version_id) in version_ids.iter().enumerate() {
        let asset_id = format!("asset-{index}");
        conn.execute(
            "INSERT INTO assets (id, project_id, type, label, created_at, updated_at) VALUES (?1, ?2, 'image', 'Seed', 'now', 'now')",
            params![asset_id, project_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO asset_versions (id, asset_id, version_number, status, file_path, thumbnail_path, sha256, original_filename, mime_type, byte_size, created_at) VALUES (?1, ?2, 1, 'candidate', 'seed.png', 'seed.webp', 'seed', 'seed.png', 'image/png', 1, 'now')",
            params![version_id, asset_id],
        ).unwrap();
    }
}

fn shot_record(scene_id: &str, id: &str, ordering: i64) -> ShotRecord {
    ShotRecord {
        id: id.into(),
        scene_id: scene_id.into(),
        ordering,
        duration_seconds: 4.0,
        keyframe_asset_version_id: None,
        intent: "Establish".into(),
        action: None,
        camera: None,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn seed_scene(conn: &Connection) -> (String, String) {
    let project_id: String = conn
        .query_row("SELECT id FROM projects", [], |row| row.get(0))
        .unwrap();
    conn.execute(
        "INSERT INTO world_scenes (id, project_id, ordinal, title, summary, created_at, updated_at) \
         VALUES ('scene-1', ?1, 0, 'Scene 001 - Ops Room', 'Tight station interior.', 'now', 'now')",
        params![project_id],
    )
    .unwrap();
    (project_id, "scene-1".to_string())
}

#[test]
fn update_shot_changes_fields_and_clearing_keyframe_stays_valid() {
    let (_temp, root) = project("shot-update");
    let conn = open_db(&root);
    let (project_id, scene_id) = seed_scene(&conn);
    repository::create_shot(&conn, &shot_record(&scene_id, "shot-1", 0)).unwrap();

    let update = ShotUpdate {
        shot_id: "shot-1".into(),
        duration_seconds: Some(6.5),
        intent: Some("Close on console".into()),
        action: Some("Mara leans in".into()),
        camera: Some("medium".into()),
    };
    let updated = repository::update_shot(&conn, &project_id, &update).unwrap();
    assert_eq!(updated.duration_seconds, 6.5);
    assert_eq!(updated.intent, "Close on console");
    assert_eq!(updated.camera.as_deref(), Some("medium"));

    seed_versions(&conn, &["kf-v1"]);
    repository::set_shot_keyframe(&conn, &project_id, "shot-1", Some("kf-v1")).unwrap();
    let shots = repository::list_shots(&conn, &scene_id).unwrap();
    assert_eq!(shots[0].keyframe_asset_version_id.as_deref(), Some("kf-v1"));

    repository::set_shot_keyframe(&conn, &project_id, "shot-1", None).unwrap();
    let shots = repository::list_shots(&conn, &scene_id).unwrap();
    assert_eq!(shots[0].keyframe_asset_version_id, None);
}

#[test]
fn delete_shot_removes_only_the_shot_row() {
    let (_temp, root) = project("shot-delete");
    let conn = open_db(&root);
    let (project_id, scene_id) = seed_scene(&conn);
    repository::create_shot(&conn, &shot_record(&scene_id, "shot-1", 0)).unwrap();
    repository::create_shot(&conn, &shot_record(&scene_id, "shot-2", 1)).unwrap();

    repository::delete_shot(&conn, &project_id, &scene_id, "shot-1").unwrap();
    let shots = repository::list_shots(&conn, &scene_id).unwrap();
    assert_eq!(shots.len(), 1);
    assert_eq!(shots[0].id, "shot-2");
    // Scene survives.
    let scene_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM world_scenes WHERE id = 'scene-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(scene_count, 1);
}

#[test]
fn reorder_shots_requires_the_exact_id_set_and_writes_contiguous_positions() {
    let (_temp, root) = project("shot-reorder");
    let mut conn = open_db(&root);
    let (project_id, scene_id) = seed_scene(&conn);
    for (id, ordering) in [("shot-1", 0), ("shot-2", 1), ("shot-3", 2)] {
        repository::create_shot(&conn, &shot_record(&scene_id, id, ordering)).unwrap();
    }

    // Move shot-3 to the front.
    let ordered = vec![
        "shot-3".to_string(),
        "shot-1".to_string(),
        "shot-2".to_string(),
    ];
    let shots = repository::reorder_shots(&mut conn, &project_id, &scene_id, &ordered).unwrap();
    assert_eq!(
        shots.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        vec!["shot-3", "shot-1", "shot-2"]
    );
    assert_eq!(
        shots.iter().map(|s| s.ordering).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );

    // Duplicates are rejected without changing state.
    let duplicate = vec![
        "shot-1".to_string(),
        "shot-1".to_string(),
        "shot-2".to_string(),
    ];
    assert!(repository::reorder_shots(&mut conn, &project_id, &scene_id, &duplicate).is_err());
    // Foreign shot ids are rejected.
    let foreign = vec![
        "shot-1".to_string(),
        "shot-2".to_string(),
        "shot-9".to_string(),
    ];
    assert!(repository::reorder_shots(&mut conn, &project_id, &scene_id, &foreign).is_err());
    // Incomplete sets are rejected.
    let partial = vec!["shot-1".to_string(), "shot-2".to_string()];
    assert!(repository::reorder_shots(&mut conn, &project_id, &scene_id, &partial).is_err());
    // State is unchanged after rejected calls.
    let shots = repository::list_shots(&conn, &scene_id).unwrap();
    assert_eq!(
        shots.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        vec!["shot-3", "shot-1", "shot-2"]
    );
}

#[test]
fn shot_ordering_is_stable_after_reopen() {
    let (_temp, root) = project("shot-reopen");
    let (project_id, scene_id) = {
        let conn = open_db(&root);
        let pair = seed_scene(&conn);
        repository::create_shot(&conn, &shot_record(&pair.1, "shot-1", 0)).unwrap();
        repository::create_shot(&conn, &shot_record(&pair.1, "shot-2", 1)).unwrap();
        pair
    };
    let conn = open_db(&root);
    let shots = repository::list_shots(&conn, &scene_id).unwrap();
    assert_eq!(
        shots.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        vec!["shot-1", "shot-2"]
    );
    let _ = project_id;
}

#[test]
fn deleting_the_middle_shot_leaves_unique_contiguous_reorderable_ordering() {
    let (_temp, root) = project("shot-delete-middle");
    let mut conn = open_db(&root);
    let (project_id, scene_id) = seed_scene(&conn);
    for (id, ordering) in [("shot-1", 0), ("shot-2", 1), ("shot-3", 2)] {
        repository::create_shot(&conn, &shot_record(&scene_id, id, ordering)).unwrap();
    }

    // Delete the middle shot.
    repository::delete_shot(&conn, &project_id, &scene_id, "shot-2").unwrap();

    // Ordering remains valid and unique (0 and 2 — no duplicates).
    let shots = repository::list_shots(&conn, &scene_id).unwrap();
    let orderings: Vec<i64> = shots.iter().map(|s| s.ordering).collect();
    assert_eq!(orderings, vec![0, 2]);
    let distinct: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT ordering) FROM scene_shots WHERE scene_id = ?1",
            rusqlite::params![scene_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(distinct, 2);

    // The scene remains reorderable into a contiguous order afterwards.
    let reordered = repository::reorder_shots(
        &mut conn,
        &project_id,
        &scene_id,
        &["shot-3".to_string(), "shot-1".to_string()],
    )
    .unwrap();
    assert_eq!(
        reordered.iter().map(|s| s.ordering).collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(
        reordered.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        vec!["shot-3", "shot-1"]
    );
}
