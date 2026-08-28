use cinematic_desktop_lib::assets::service::AssetService;
use cinematic_desktop_lib::canon::model::CanonEntityType;
use cinematic_desktop_lib::canon::service::CanonService;
use cinematic_desktop_lib::canon::tbd;
use cinematic_desktop_lib::cinema::model::CinemaCompileInput;
use cinematic_desktop_lib::cinema::service::CinemaService;
use cinematic_desktop_lib::integration::readiness::{get_project_overview, ReadinessStatus};
use cinematic_desktop_lib::project::service::ProjectService;
use std::path::{Path, PathBuf};
use tempfile::{tempdir, TempDir};

struct ProjectFixture {
    _temp: TempDir,
    root: PathBuf,
}

impl ProjectFixture {
    fn new() -> Self {
        let temp = tempdir().unwrap();
        let root = temp.path().join("red-door");
        ProjectService::create(&root, "Red Door").unwrap();
        Self { _temp: temp, root }
    }

    fn image(&self, name: &str, pixel: [u8; 4]) -> PathBuf {
        let path = self.root.join(name);
        image::RgbaImage::from_pixel(16, 16, image::Rgba(pixel))
            .save(&path)
            .unwrap();
        path
    }

    fn canonical_asset(
        &self,
        asset_type: &str,
        label: &str,
        owner_entity_id: Option<String>,
        pixel: [u8; 4],
    ) -> (String, String) {
        let asset =
            AssetService::create_asset(&self.root, asset_type, label, owner_entity_id).unwrap();
        let image = self.image(&format!("{label}.png"), pixel);
        let version =
            AssetService::import_asset_version(&self.root, &asset.id, &image, None).unwrap();
        AssetService::promote_asset_version(&self.root, &version.id).unwrap();
        (asset.id, version.id)
    }

    fn character(&self) -> String {
        CanonService::create_entity(&self.root, CanonEntityType::Character, "Mara Keene")
            .unwrap()
            .id
    }

    fn ready_scene(&self) -> ReadyScene {
        let character_id = self.character();
        self.canonical_asset(
            "face_lock",
            "Mara Face",
            Some(character_id.clone()),
            [1, 2, 3, 255],
        );
        let (look_asset_id, look_version_id) = self.canonical_asset(
            "outfit",
            "Mara Look",
            Some(character_id.clone()),
            [4, 5, 6, 255],
        );
        let (_, sheet_version_id) = self.canonical_asset(
            "character_sheet",
            "Mara Sheet",
            Some(character_id.clone()),
            [7, 8, 9, 255],
        );
        let (_, world_version_id) =
            self.canonical_asset("world_plate", "Station World", None, [10, 11, 12, 255]);

        for key in ["speech", "movement", "stillness"] {
            let section = CanonService::upsert_section(
                &self.root,
                &character_id,
                key,
                serde_json::json!({ "text": format!("locked {key}") }),
                None,
            )
            .unwrap();
            CanonService::lock_section(&self.root, &section.id, None).unwrap();
        }
        let visual_locks = CanonService::upsert_section(
            &self.root,
            &character_id,
            "visual_locks",
            serde_json::json!({ "locks": [] }),
            None,
        )
        .unwrap();
        CanonService::lock_section(&self.root, &visual_locks.id, None).unwrap();

        let scene =
            CinemaService::create_scene(&self.root, "Scene 001", Some(world_version_id), None)
                .unwrap();
        CinemaService::add_character_to_scene(
            &self.root,
            &scene.id,
            &character_id,
            &look_version_id,
            Some(sheet_version_id),
        )
        .unwrap();
        CinemaService::create_shot(&self.root, &scene.id, None, 4.0, "Mara enters", None, None)
            .unwrap();

        ReadyScene {
            character_id,
            look_asset_id,
            scene_id: scene.id,
        }
    }
}

struct ReadyScene {
    character_id: String,
    look_asset_id: String,
    scene_id: String,
}

fn next_title(root: &Path) -> String {
    get_project_overview(root)
        .unwrap()
        .readiness
        .next_action
        .unwrap()
        .title
}

#[test]
fn empty_project_recommends_story_canon() {
    let fixture = ProjectFixture::new();
    assert_eq!(next_title(&fixture.root), "Story Canon");
}

#[test]
fn character_without_canonical_face_recommends_face_lock() {
    let fixture = ProjectFixture::new();
    fixture.character();
    assert_eq!(next_title(&fixture.root), "Face Lock");
}

#[test]
fn newest_face_candidate_does_not_count_as_canonical() {
    let fixture = ProjectFixture::new();
    let character_id = fixture.character();
    let face =
        AssetService::create_asset(&fixture.root, "face_lock", "Mara Face", Some(character_id))
            .unwrap();
    let candidate = fixture.image("mara-candidate.png", [2, 3, 4, 255]);
    AssetService::import_asset_version(&fixture.root, &face.id, &candidate, None).unwrap();

    assert_eq!(next_title(&fixture.root), "Face Lock");
}

#[test]
fn canonical_face_without_look_recommends_character_look() {
    let fixture = ProjectFixture::new();
    let character_id = fixture.character();
    fixture.canonical_asset("face_lock", "Mara Face", Some(character_id), [1, 2, 3, 255]);
    assert_eq!(next_title(&fixture.root), "Character Look");
}

#[test]
fn look_without_sheet_recommends_sheet() {
    let fixture = ProjectFixture::new();
    let character_id = fixture.character();
    fixture.canonical_asset(
        "face_lock",
        "Mara Face",
        Some(character_id.clone()),
        [1, 2, 3, 255],
    );
    fixture.canonical_asset("outfit", "Mara Look", Some(character_id), [4, 5, 6, 255]);
    assert_eq!(next_title(&fixture.root), "Character Sheet");
}

#[test]
fn complete_character_pipeline_without_world_recommends_world_plate() {
    let fixture = ProjectFixture::new();
    let character_id = fixture.character();
    fixture.canonical_asset(
        "face_lock",
        "Mara Face",
        Some(character_id.clone()),
        [1, 2, 3, 255],
    );
    fixture.canonical_asset(
        "outfit",
        "Mara Look",
        Some(character_id.clone()),
        [4, 5, 6, 255],
    );
    fixture.canonical_asset(
        "character_sheet",
        "Mara Sheet",
        Some(character_id),
        [7, 8, 9, 255],
    );
    assert_eq!(next_title(&fixture.root), "World Plate");
}

#[test]
fn world_and_complete_character_without_scene_recommends_scene() {
    let fixture = ProjectFixture::new();
    let character_id = fixture.character();
    fixture.canonical_asset(
        "face_lock",
        "Mara Face",
        Some(character_id.clone()),
        [1, 2, 3, 255],
    );
    fixture.canonical_asset(
        "outfit",
        "Mara Look",
        Some(character_id.clone()),
        [4, 5, 6, 255],
    );
    fixture.canonical_asset(
        "character_sheet",
        "Mara Sheet",
        Some(character_id),
        [7, 8, 9, 255],
    );
    fixture.canonical_asset("world_plate", "Station World", None, [10, 11, 12, 255]);
    assert_eq!(next_title(&fixture.root), "Scene");
}

#[test]
fn protected_open_tbd_for_scene_character_reports_blocked() {
    let fixture = ProjectFixture::new();
    let ready = fixture.ready_scene();
    tbd::create(
        &fixture.root,
        Some(&ready.character_id),
        None,
        "Mara's true motive",
        None,
        true,
    )
    .unwrap();

    let overview = get_project_overview(&fixture.root).unwrap();
    assert_eq!(overview.readiness.status, ReadinessStatus::Blocked);
    assert_eq!(
        overview.readiness.next_action.unwrap().title,
        "Resolve protected TBD"
    );
}

#[test]
fn valid_scene_without_compilation_recommends_cinema_compilation() {
    let fixture = ProjectFixture::new();
    fixture.ready_scene();
    assert_eq!(next_title(&fixture.root), "Cinema Compilation");
}

#[test]
fn completed_cinema_compilation_reports_complete_production_path() {
    let fixture = ProjectFixture::new();
    let ready = fixture.ready_scene();
    CinemaService::compile_scene(
        &fixture.root,
        CinemaCompileInput {
            scene_id: ready.scene_id,
            total_duration_seconds: 4.0,
            shot_count: None,
        },
    )
    .unwrap();

    let overview = get_project_overview(&fixture.root).unwrap();
    assert_eq!(overview.readiness.status, ReadinessStatus::Complete);
    assert!(overview.readiness.next_action.is_none());
}

#[test]
fn superseded_scene_reference_remains_recorded_as_ready_history() {
    let fixture = ProjectFixture::new();
    let ready = fixture.ready_scene();
    let updated = fixture.image("mara-look-revision.png", [99, 5, 6, 255]);
    let replacement =
        AssetService::import_asset_version(&fixture.root, &ready.look_asset_id, &updated, None)
            .unwrap();
    AssetService::promote_asset_version(&fixture.root, &replacement.id).unwrap();

    let overview = get_project_overview(&fixture.root).unwrap();
    let scene_step = overview
        .readiness
        .steps
        .iter()
        .find(|step| step.id == "scene")
        .unwrap();
    assert_eq!(scene_step.status, ReadinessStatus::Complete);
    assert_eq!(
        overview.readiness.next_action.unwrap().title,
        "Cinema Compilation"
    );
}
