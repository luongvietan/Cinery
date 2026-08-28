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
    // Integration tests open the same file the service just bootstrapped;
    // FK enforcement is session-scoped, so re-assert it here.
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    conn
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
        intent: "Establish the ops room".into(),
        action: Some("Mara scans the console".into()),
        camera: Some("wide".into()),
        generated_video_asset_version_id: None,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn insert_canon_character(conn: &Connection, project_id: &str, id: &str) {
    conn.execute(
        "INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at) \
         VALUES (?1, ?2, 'character', 'Mara Keene', 'mara-keene', 'now', 'now')",
        params![id, project_id],
    )
    .unwrap();
}

fn insert_canonical_asset_version(conn: &Connection, project_id: &str, id: &str) {
    conn.execute(
        "INSERT INTO assets (id, project_id, type, label, created_at, updated_at) \
         VALUES (?1, ?2, 'outfit', 'Look', 'now', 'now')",
        params![id, project_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO asset_versions (id, asset_id, version_number, status, file_path, \
         thumbnail_path, sha256, original_filename, mime_type, byte_size, created_at) \
         VALUES (?1, ?1, 1, 'canonical', 'v.png', 't.png', \
         'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', \
         'v.png', 'image/png', 1, 'now')",
        params![id],
    )
    .unwrap();
}
#[test]
fn creates_scene_and_shots_with_ordering_uniqueness() {
    let (_temp, root) = project("Red Door");
    let conn = open_db(&root);
    let project_id: String = conn
        .query_row("SELECT id FROM projects LIMIT 1", [], |row| row.get(0))
        .unwrap();

    let scene = scene_record(&project_id, "scene-1");
    repository::create_scene(&conn, &scene).unwrap();

    let fetched = repository::get_scene(&conn, &project_id, "scene-1").unwrap();
    assert_eq!(fetched.title, "Scene 001 - Ops Room");

    // Project mismatch must not leak another project's scene.
    assert!(repository::get_scene(&conn, "other-project", "scene-1").is_err());

    repository::create_shot(&conn, &shot_record("scene-1", "shot-1", 0)).unwrap();
    repository::create_shot(&conn, &shot_record("scene-1", "shot-2", 1)).unwrap();

    // Duplicate (scene_id, ordering) is rejected.
    assert!(repository::create_shot(&conn, &shot_record("scene-1", "shot-3", 0)).is_err());

    let shots = repository::list_shots(&conn, "scene-1").unwrap();
    assert_eq!(shots.len(), 2);
    assert_eq!(shots[0].ordering, 0);
    assert_eq!(shots[1].ordering, 1);

    let scenes = repository::list_scenes(&conn, &project_id).unwrap();
    assert_eq!(scenes.len(), 1);
    assert!(repository::list_scenes(&conn, "other-project").unwrap().is_empty());
}

#[test]
fn scene_characters_and_props_require_existing_rows() {
    let (_temp, root) = project("Red Door");
    let conn = open_db(&root);
    let project_id: String = conn
        .query_row("SELECT id FROM projects LIMIT 1", [], |row| row.get(0))
        .unwrap();

    repository::create_scene(&conn, &scene_record(&project_id, "scene-1")).unwrap();

    // FK: unknown character entity is rejected.
    let missing_character = SceneCharacterRecord {
        scene_id: "scene-1".into(),
        character_entity_id: "no-such-character".into(),
        look_asset_version_id: "look-v1".into(),
        sheet_asset_version_id: None,
        display_order: 0,
    };
    assert!(repository::add_scene_character(&conn, &missing_character).is_err());

    insert_canon_character(&conn, &project_id, "character-1");
    insert_canonical_asset_version(&conn, &project_id, "look-v1");
    insert_canonical_asset_version(&conn, &project_id, "sheet-v1");

    let with_look = SceneCharacterRecord {
        scene_id: "scene-1".into(),
        character_entity_id: "character-1".into(),
        look_asset_version_id: "look-v1".into(),
        sheet_asset_version_id: Some("sheet-v1".into()),
        display_order: 0,
    };
    repository::add_scene_character(&conn, &with_look).unwrap();

    // FK: unknown prop plate version is rejected.
    assert!(repository::add_scene_prop(
        &conn,
        &ScenePropRecord {
            scene_id: "scene-1".into(),
            prop_asset_version_id: "no-such-prop".into(),
            display_order: 0,
        }
    )
    .is_err());
}

#[test]
fn compilations_round_trip_with_scene_foreign_key() {
    let (_temp, root) = project("Red Door");
    let conn = open_db(&root);
    let project_id: String = conn
        .query_row("SELECT id FROM projects LIMIT 1", [], |row| row.get(0))
        .unwrap();

    // FK: compilation must reference a real scene.
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

    repository::create_scene(&conn, &scene_record(&project_id, "scene-1")).unwrap();
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

