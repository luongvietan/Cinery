//! Cinema workspace CRUD repository tests: rename, world pinning, cast and
//! prop relationship updates, shot updates, transactional reorder, and
//! keyframe assignment. All mutations are project-scoped.

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

/// Seeds minimal, FK-valid asset version rows so relationship pins satisfy
/// the schema's foreign keys.
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

fn scene_record(project_id: &str, id: &str) -> SceneRecord {
    SceneRecord {
        id: id.into(),
        project_id: project_id.into(),
        title: "Scene 001 - Ops Room".into(),
        world_asset_version_id: None,
        canon_notes: Some("Tight station interior.".into()),
        created_at: "now".into(),
        updated_at: "now".into(),
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
        generated_video_asset_version_id: None,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn seed_scene(conn: &Connection) -> (String, String) {
    let project_id: String = conn
        .query_row("SELECT id FROM projects", [], |row| row.get(0))
        .unwrap();
    let scene = scene_record(&project_id, "scene-1");
    repository::create_scene(conn, &scene).unwrap();
    (project_id, scene.id)
}

#[test]
fn rename_scene_updates_title_within_project_scope() {
    let (_temp, root) = project("rename-scene");
    let conn = open_db(&root);
    let (project_id, scene_id) = seed_scene(&conn);

    let renamed = repository::rename_scene(&conn, &project_id, &scene_id, "Renamed Scene").unwrap();
    assert_eq!(renamed.title, "Renamed Scene");

    // Foreign scene id is rejected as not found.
    let foreign = repository::rename_scene(&conn, &project_id, "scene-other", "Nope");
    assert!(foreign.is_err());
}

#[test]
fn set_scene_world_pins_and_clears_the_exact_version() {
    let (_temp, root) = project("set-world");
    let conn = open_db(&root);
    let (project_id, scene_id) = seed_scene(&conn);

    seed_versions(&conn, &["world-v1"]);
    repository::set_scene_world(&conn, &project_id, &scene_id, Some("world-v1")).unwrap();
    let scene = repository::get_scene(&conn, &project_id, &scene_id).unwrap();
    assert_eq!(scene.world_asset_version_id.as_deref(), Some("world-v1"));

    repository::set_scene_world(&conn, &project_id, &scene_id, None).unwrap();
    let scene = repository::get_scene(&conn, &project_id, &scene_id).unwrap();
    assert_eq!(scene.world_asset_version_id, None);
}

#[test]
fn cast_relationships_can_be_updated_and_removed_without_deleting_canon() {
    let (_temp, root) = project("cast-crud");
    let conn = open_db(&root);
    let (project_id, scene_id) = seed_scene(&conn);
    conn.execute(
        "INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at) VALUES ('mara', ?1, 'character', 'Mara', 'mara', 'now', 'now')",
        params![project_id],
    ).unwrap();
    seed_versions(&conn, &["look-v1", "look-v2", "sheet-v1"]);
    repository::add_scene_character(&conn, &SceneCharacterRecord {
        scene_id: scene_id.clone(),
        character_entity_id: "mara".into(),
        look_asset_version_id: "look-v1".into(),
        sheet_asset_version_id: None,
        display_order: 0,
    }).unwrap();

    // Update look and add a sheet pin.
    repository::update_scene_character(&conn, &project_id, &scene_id, "mara", Some("look-v2"), Some("sheet-v1")).unwrap();
    let cast = repository::list_scene_characters(&conn, &scene_id).unwrap();
    assert_eq!(cast.len(), 1);
    assert_eq!(cast[0].look_asset_version_id, "look-v2");
    assert_eq!(cast[0].sheet_asset_version_id.as_deref(), Some("sheet-v1"));

    // Removing the cast record keeps the canon entity and the scene.
    repository::remove_scene_character(&conn, &project_id, &scene_id, "mara").unwrap();
    assert!(repository::list_scene_characters(&conn, &scene_id).unwrap().is_empty());
    let entity_count: i64 = conn.query_row("SELECT COUNT(*) FROM canon_entities WHERE id = 'mara'", [], |r| r.get(0)).unwrap();
    let scene_count: i64 = conn.query_row("SELECT COUNT(*) FROM scenes WHERE id = 'scene-1'", [], |r| r.get(0)).unwrap();
    assert_eq!(entity_count, 1);
    assert_eq!(scene_count, 1);
}

#[test]
fn props_can_be_removed_by_version_without_touching_assets() {
    let (_temp, root) = project("prop-crud");
    let conn = open_db(&root);
    let (project_id, scene_id) = seed_scene(&conn);
    seed_versions(&conn, &["prop-v1"]);
    repository::add_scene_prop(&conn, &ScenePropRecord {
        scene_id: scene_id.clone(),
        prop_asset_version_id: "prop-v1".into(),
        display_order: 0,
    }).unwrap();

    repository::remove_scene_prop(&conn, &project_id, &scene_id, "prop-v1").unwrap();
    assert!(repository::list_scene_props(&conn, &scene_id).unwrap().is_empty());
    // The seeded source asset itself survives the relationship deletion.
    let asset_count: i64 = conn.query_row("SELECT COUNT(*) FROM assets", [], |r| r.get(0)).unwrap();
    assert!(asset_count >= 1);
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
    let scene_count: i64 = conn.query_row("SELECT COUNT(*) FROM scenes WHERE id = 'scene-1'", [], |r| r.get(0)).unwrap();
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
    let ordered = vec!["shot-3".to_string(), "shot-1".to_string(), "shot-2".to_string()];
    let shots = repository::reorder_shots(&mut conn, &project_id, &scene_id, &ordered).unwrap();
    assert_eq!(shots.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), vec!["shot-3", "shot-1", "shot-2"]);
    assert_eq!(shots.iter().map(|s| s.ordering).collect::<Vec<_>>(), vec![0, 1, 2]);

    // Duplicates are rejected without changing state.
    let duplicate = vec!["shot-1".to_string(), "shot-1".to_string(), "shot-2".to_string()];
    assert!(repository::reorder_shots(&mut conn, &project_id, &scene_id, &duplicate).is_err());
    // Foreign shot ids are rejected.
    let foreign = vec!["shot-1".to_string(), "shot-2".to_string(), "shot-9".to_string()];
    assert!(repository::reorder_shots(&mut conn, &project_id, &scene_id, &foreign).is_err());
    // Incomplete sets are rejected.
    let partial = vec!["shot-1".to_string(), "shot-2".to_string()];
    assert!(repository::reorder_shots(&mut conn, &project_id, &scene_id, &partial).is_err());
    // State is unchanged after rejected calls.
    let shots = repository::list_shots(&conn, &scene_id).unwrap();
    assert_eq!(shots.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), vec!["shot-3", "shot-1", "shot-2"]);
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
    assert_eq!(shots.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), vec!["shot-1", "shot-2"]);
    let _ = project_id;
}
