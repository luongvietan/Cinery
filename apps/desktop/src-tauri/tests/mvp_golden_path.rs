use cinematic_desktop_lib::assets::service::AssetService;
use cinematic_desktop_lib::canon::model::CanonEntityType;
use cinematic_desktop_lib::canon::model::CanonEntityType as EntityType;
use cinematic_desktop_lib::canon::service::CanonService;
use cinematic_desktop_lib::cinema::model::CinemaCompileInput;
use cinematic_desktop_lib::cinema::service::CinemaService;
use cinematic_desktop_lib::integration::health::scan_project;
use cinematic_desktop_lib::integration::provenance::get_provenance_graph;
use cinematic_desktop_lib::integration::readiness::get_project_overview;
use cinematic_desktop_lib::project::service::ProjectService;
use cinematic_desktop_lib::scenes::service::SceneService;
use cinematic_desktop_lib::worlds::service::WorldService;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Deterministic Mara fixture: one coherent project exercising the major
/// P0-P8 boundaries. Operations use service-layer APIs directly (no Tauri
/// IPC) so the run is deterministic and provider-free.
#[test]
fn complete_mara_cinematic_journey() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("mara-mvp");

    // ── Step 1: Create project ──
    let project = ProjectService::create(&root, "Mara MVP").unwrap();

    // ── Step 2: Story Canon (Mara Character) ──
    let mara =
        CanonService::create_entity(&root, CanonEntityType::Character, "Mara Keene").unwrap();

    // ── Step 2b: Lock required behavioral canon sections ──
    for key in ["speech", "movement", "stillness"] {
        let section = CanonService::upsert_section(
            &root,
            &mara.id,
            key,
            serde_json::json!({ "text": format!("locked {key}") }),
            None,
        )
        .unwrap();
        CanonService::lock_section(&root, &section.id, None).unwrap();
    }
    let visual_locks = CanonService::upsert_section(
        &root,
        &mara.id,
        "visual_locks",
        serde_json::json!({ "locks": [] }),
        None,
    )
    .unwrap();
    CanonService::lock_section(&root, &visual_locks.id, None).unwrap();

    // ── Step 3: Canonical visual locks (Face / Look / Sheet) ──
    let (face_asset_id, face_version_id) = canonical_asset(
        &root,
        "face_lock",
        "Mara Face",
        Some(mara.id.clone()),
        [1, 2, 3, 255],
    );
    let (_look_asset_id, look_version_id) = canonical_asset(
        &root,
        "outfit",
        "Mara Look",
        Some(mara.id.clone()),
        [4, 5, 6, 255],
    );
    let (_sheet_asset_id, sheet_version_id) = canonical_asset(
        &root,
        "character_sheet",
        "Mara Sheet",
        Some(mara.id.clone()),
        [7, 8, 9, 255],
    );

    // ── Step 4: World (production entity over a Canon Location) ──
    let (_world_asset_id, _world_version_id) = canonical_asset(
        &root,
        "world_plate",
        "Station World",
        None,
        [10, 11, 12, 255],
    );
    let location = CanonService::create_entity(&root, EntityType::Location, "The Station").unwrap();
    let world = WorldService::create_world(&root, &location.id).unwrap();
    {
        let plate_source = image(&root, "world-plate.png", [13, 14, 15, 255]);
        let plate_version = AssetService::import_asset_version(
            &root,
            &world.world_plate_asset_id,
            &plate_source,
            None,
        )
        .unwrap();
        AssetService::promote_asset_version(&root, &plate_version.id).unwrap();
    }

    // ── Step 5: Assemble Scene with pinned exact versions ──
    let scene =
        SceneService::create_scene(&root, "Scene 001", "Mara returns to the station").unwrap();
    SceneService::assign_scene_world(&root, &scene.id, &world.id).unwrap();
    SceneService::add_scene_character(
        &root,
        &scene.id,
        &mara.id,
        &look_version_id,
        Some(sheet_version_id.as_str()),
        None,
    )
    .unwrap();
    let scene = SceneService::get_scene(&root, &scene.id).unwrap();
    let world_version_id = scene.world_asset_version_id.clone().unwrap();

    // ── Step 6: Add Shot (4s, inside the 8s runtime budget) ──
    let shot =
        CinemaService::create_shot(&root, &scene.id, None, 4.0, "Mara enters", None, None).unwrap();
    assert_eq!(shot.duration_seconds, 4.0);

    // ── Step 7: Readiness derivation ──
    let overview = get_project_overview(&root).unwrap();
    assert_eq!(
        overview.readiness.next_action.as_ref().unwrap().title,
        "Cinema Compilation"
    );

    // ── Step 8: Compile 8-second Cinema Prompt ──
    let compilation = CinemaService::compile_scene(
        &root,
        CinemaCompileInput {
            scene_id: scene.id.clone(),
            total_duration_seconds: 4.0,
            shot_count: None,
        },
    )
    .unwrap();
    assert!(root.join(&compilation.export_path).exists());

    // ── Step 9: Protected TBD blocks compilation after promotion guard ──
    cinematic_desktop_lib::canon::tbd::create(
        &root,
        Some(&mara.id),
        None,
        "Mara's true motive",
        None,
        true,
    )
    .unwrap();
    let protected = cinematic_desktop_lib::canon::tbd::list_open_protected(&root).unwrap();
    assert_eq!(protected.len(), 1, "protected TBD must remain open");

    let blocked = get_project_overview(&root).unwrap();
    assert!(matches!(
        blocked.readiness.status,
        cinematic_desktop_lib::integration::readiness::ReadinessStatus::Blocked
    ));

    // ── Step 10: Protected TBD resolution is explicit ──
    cinematic_desktop_lib::canon::tbd::resolve(&root, &protected[0].id, "Resolved").unwrap();
    let resolved = get_project_overview(&root).unwrap();
    assert!(!matches!(
        resolved.readiness.status,
        cinematic_desktop_lib::integration::readiness::ReadinessStatus::Blocked
    ));

    // ── Step 11: Scene references are pinned: promoting World V02 must not
    //     silently rewrite the Scene's exact V01 reference. ──
    let (_asset_v02, world_v02_id) = canonical_asset(
        &root,
        "world_plate",
        "Station World V02",
        None,
        [20, 21, 22, 255],
    );
    let scene_after = SceneService::get_scene(&root, &scene.id).unwrap();
    assert_eq!(
        scene_after.world_asset_version_id.as_deref(),
        Some(world_version_id.as_str()),
        "scene must still reference the exact World V01 version"
    );
    assert_ne!(world_v02_id, world_version_id);

    // ── Step 12: Provenance traversal ──
    let prov = get_provenance_graph(&root, "asset_version", &face_version_id).unwrap();
    assert!(
        prov.nodes.iter().any(|node| node.id == face_version_id),
        "provenance must contain the target node"
    );

    // A staged scene pins exact versions, so its provenance has edges to
    // the world and character-look versions it references.
    let scene_prov = get_provenance_graph(&root, "scene", &scene.id).unwrap();
    assert!(
        scene_prov
            .edges
            .iter()
            .any(|edge| edge.relation == "USES_WORLD"),
        "scene provenance must link its pinned world version"
    );

    // ── Step 13: Health scan reports no integrity issues ──
    let issues = scan_project(&root).unwrap();
    assert!(
        issues.is_empty(),
        "expected no health issues, got: {:?}",
        issues
    );

    // ── Step 14: Close project, reopen, verify exact state remains ──
    drop(project);
    drop(overview);
    drop(blocked);

    let reopened = ProjectService::open(&root).unwrap();
    assert_eq!(reopened.name, "Mara MVP");

    let reloaded_face = AssetService::get_asset_with_versions(&root, &face_asset_id).unwrap();
    assert_eq!(
        reloaded_face.asset.canonical_version_id.as_deref(),
        Some(face_version_id.as_str())
    );

    let reloaded_scene = SceneService::get_scene(&root, &scene.id).unwrap();
    assert_eq!(reloaded_scene.title, "Scene 001");
    assert_eq!(
        reloaded_scene.world_asset_version_id.as_deref(),
        Some(world_version_id.as_str())
    );
}

fn canonical_asset(
    root: &Path,
    asset_type: &str,
    label: &str,
    owner_entity_id: Option<String>,
    pixel: [u8; 4],
) -> (String, String) {
    let asset = AssetService::create_asset(root, asset_type, label, owner_entity_id).unwrap();
    let img = image(root, &format!("{label}.png"), pixel);
    let version = AssetService::import_asset_version(root, &asset.id, &img, None).unwrap();
    AssetService::promote_asset_version(root, &version.id).unwrap();
    (asset.id, version.id)
}

fn image(root: &Path, name: &str, pixel: [u8; 4]) -> PathBuf {
    let path = root.join(name);
    image::RgbaImage::from_pixel(16, 16, image::Rgba(pixel))
        .save(&path)
        .unwrap();
    path
}
