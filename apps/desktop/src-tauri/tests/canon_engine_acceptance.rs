use cinematic_desktop_lib::canon::model::CanonEntityType;
use cinematic_desktop_lib::canon::service::CanonService;
use cinematic_desktop_lib::canon::{export, tbd};
use cinematic_desktop_lib::project::service::ProjectService;
use std::fs;
use tempfile::tempdir;

#[test]
fn canon_survives_restart_with_locks_revisions_queries_and_deterministic_export() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("red-door");
    ProjectService::create(&root, "Red Door").unwrap();
    let singletons = CanonService::ensure_singletons(&root).unwrap();

    let premise = CanonService::upsert_section(
        &root,
        &singletons.story.id,
        "premise",
        serde_json::json!({"text":"A lone radio operator receives her future voice."}),
        None,
    )
    .unwrap();
    CanonService::lock_section(&root, &premise.id, None).unwrap();
    let thesis = CanonService::upsert_section(
        &root,
        &singletons.story.id,
        "thesis",
        serde_json::json!({"text":"The unknown must remain unknown."}),
        None,
    )
    .unwrap();
    assert_eq!(thesis.status, "draft");

    let mara =
        CanonService::create_entity(&root, CanonEntityType::Character, "Mara Keene").unwrap();
    let character_id = mara.id.clone();
    let role = CanonService::upsert_section(
        &root,
        &mara.id,
        "role_tag",
        serde_json::json!({"text":"The Verifier"}),
        None,
    )
    .unwrap();
    CanonService::lock_section(&root, &role.id, None).unwrap();
    let function = CanonService::upsert_section(
        &root,
        &mara.id,
        "function",
        serde_json::json!({"text":"Tests whether the signal can be trusted."}),
        None,
    )
    .unwrap();
    CanonService::lock_section(&root, &function.id, None).unwrap();
    let visual_locks = CanonService::upsert_section(&root, &mara.id, "visual_locks", serde_json::json!({"locks":[
        {"id":"scar","key":"right_eyebrow_scar","description":"Small healed linear scar.","severity":"required","validatorHint":"Character-right appears viewer-left."},
        {"id":"no-bangs","key":"no_bangs","description":"Forehead remains visible.","severity":"important","validatorHint":null}
    ]}), None).unwrap();
    CanonService::lock_section(&root, &visual_locks.id, None).unwrap();

    let station =
        CanonService::create_entity(&root, CanonEntityType::Location, "The Station").unwrap();
    let geography = CanonService::upsert_section(
        &root,
        &station.id,
        "geography",
        serde_json::json!({"text":"A coastal relay station below the old lighthouse."}),
        None,
    )
    .unwrap();
    CanonService::lock_section(&root, &geography.id, None).unwrap();
    let world_rule = CanonService::create_entity(
        &root,
        CanonEntityType::WorldRule,
        "Anomaly Uses Radio Infrastructure",
    )
    .unwrap();
    let rule = CanonService::upsert_section(&root, &world_rule.id, "rule", serde_json::json!({"text":"The anomaly manifests only through existing radio infrastructure."}), None).unwrap();
    CanonService::lock_section(&root, &rule.id, None).unwrap();
    let production_rule = serde_json::json!({"rules":[{"id":"unknown-canon","title":"Unknown canon stays unknown","body":"Anything marked TBD must not be resolved by downstream generation."}]});
    let production = CanonService::upsert_section(
        &root,
        &singletons.production_rules.id,
        "rules",
        production_rule,
        None,
    )
    .unwrap();
    CanonService::lock_section(&root, &production.id, None).unwrap();
    let red_door = tbd::create(
        &root,
        None,
        None,
        "What is behind the red door?",
        Some("No generation may reveal it before canon intentionally resolves it.".into()),
        true,
    )
    .unwrap();

    let first_export = export::export_story_bible(&root).unwrap();
    let first_bytes = fs::read(root.join("canon/story-bible.md")).unwrap();
    drop(singletons);
    drop(premise);
    drop(thesis);
    drop(mara);
    drop(station);
    drop(world_rule);
    drop(red_door);
    ProjectService::open(&root).unwrap();

    let reopened_story = CanonService::ensure_singletons(&root).unwrap().story;
    let reopened = CanonService::get_entity(&root, &reopened_story.id).unwrap();
    let reopened_premise = reopened
        .sections
        .iter()
        .find(|section| section.key == "premise")
        .unwrap();
    assert_eq!(reopened_premise.status, "locked");
    assert!(matches!(
        CanonService::upsert_section(
            &root,
            &reopened_story.id,
            "premise",
            serde_json::json!({"text":"Not allowed"}),
            None
        ),
        Err(cinematic_desktop_lib::error::AppError::CanonSectionLocked)
    ));
    CanonService::unlock_section(&root, &reopened_premise.id, None).unwrap();
    CanonService::upsert_section(
        &root,
        &reopened_story.id,
        "premise",
        serde_json::json!({"text":"A revised premise."}),
        None,
    )
    .unwrap();
    let history = CanonService::list_section_revisions(&root, &reopened_premise.id).unwrap();
    assert_eq!(
        history
            .iter()
            .map(|revision| revision.change_kind.as_str())
            .collect::<Vec<_>>(),
        vec!["edit", "unlock", "lock", "create"]
    );
    assert_eq!(
        CanonService::get_locked_character_visual_locks(&root, &character_id)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        CanonService::list_locked_world_rules(&root).unwrap().len(),
        1
    );
    assert_eq!(
        CanonService::get_locked_production_rules(&root)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(tbd::list_open_protected(&root).unwrap().len(), 1);

    let second_export = export::export_story_bible(&root).unwrap();
    let second_bytes = fs::read(root.join("canon/story-bible.md")).unwrap();
    export::export_story_bible(&root).unwrap();
    let third_bytes = fs::read(root.join("canon/story-bible.md")).unwrap();
    assert_eq!(first_export.relative_path, second_export.relative_path);
    assert_ne!(
        first_bytes, second_bytes,
        "the explicit post-restart premise edit should be reflected"
    );
    assert_eq!(second_bytes, third_bytes);
}
