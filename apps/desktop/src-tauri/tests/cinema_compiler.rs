use cinematic_desktop_lib::cinema::compiler;
use cinematic_desktop_lib::cinema::model::{SceneRef, ShotRecord};

mod support;

use support::compilable_scene;

fn scene_ref(
    setup: &support::CompiledScene,
) -> SceneRef {
    SceneRef {
        id: setup.scene.id.clone(),
        project_id: setup.scene.project_id.clone(),
        title: setup.scene.title.clone(),
        summary: setup.scene.summary.clone(),
    }
}

#[test]
fn compiles_8s_two_shot_with_behavior_and_world_continuity() {
    let setup = compilable_scene();

    let prompt = compiler::compile(
        &scene_ref(&setup),
        "comp-1",
        8.0,
        None,
        &setup.behavioral_locks,
        &setup.world_continuity,
        &setup.visual_locks,
        &setup.shots,
        &[],
    )
    .unwrap();

    assert_eq!(prompt.total_duration_seconds, 8.0);
    assert_eq!(prompt.shots.len(), 2);
    let sum: f64 = prompt.shots.iter().map(|shot| shot.duration_seconds).sum();
    assert!((sum - 8.0).abs() < 1e-9);
    assert_eq!(prompt.time_budget, vec![4.0, 4.0]);

    assert_eq!(
        prompt.behavioral_locks.speech.as_deref(),
        Some("locked speech")
    );
    assert_eq!(
        prompt.behavioral_locks.movement.as_deref(),
        Some("locked movement")
    );
    assert_eq!(
        prompt.behavioral_locks.stillness.as_deref(),
        Some("locked stillness")
    );

    assert_eq!(
        prompt.world_continuity.plate_asset_version_id,
        setup.scene.world_asset_version_id
    );

    // Every shot carries the sorted visual locks and a continuity note that
    // references the canonical look and world plate.
    for (index, shot) in prompt.shots.iter().enumerate() {
        assert_eq!(shot.order, index);
        let keys: Vec<&str> = shot
            .subject_locks
            .iter()
            .map(|lock| lock.key.as_str())
            .collect();
        assert_eq!(keys, vec!["left_wrist_watch", "right_eyebrow_scar"]);
        let note = shot.continuity_note.as_deref().unwrap();
        assert!(note.contains("canonical look"));
        assert!(note.contains("world plate"));
    }

    let text = &prompt.provider_prompt;
    assert!(text.contains("locked speech"));
    assert!(text.contains("locked movement"));
    assert!(text.contains("locked stillness"));
    assert!(text.contains("Establish the ops room"));
    assert!(text.contains("Close on the console"));
    assert!(text.contains("World Continuity"));
    assert!(text.contains("8s"));
    assert!(text.contains("comp-1"));
}

#[test]
fn compiles_deterministically_and_scrubs_open_tbd_topics() {
    let setup = compilable_scene();
    let compile = |shots: &[ShotRecord], topics: &[String]| {
        compiler::compile(
            &scene_ref(&setup),
            "comp-1",
            8.0,
            None,
            &setup.behavioral_locks,
            &setup.world_continuity,
            &setup.visual_locks,
            shots,
            topics,
        )
        .unwrap()
    };

    let first = compile(&setup.shots, &[]);
    let second = compile(&setup.shots, &[]);
    assert_eq!(first.provider_prompt, second.provider_prompt);
    assert_eq!(
        serde_json::to_string(&first).unwrap(),
        serde_json::to_string(&second).unwrap()
    );

    // An open (unprotected) TBD topic that leaked into shot action text is
    // scrubbed from the prompt deterministically.
    let mut shots = setup.shots.clone();
    shots[0].action = Some("What is behind the red door? Mara glances over".into());
    let scrubbed = compile(&shots, &["What is behind the red door?".to_string()]);
    assert!(!scrubbed
        .provider_prompt
        .contains("What is behind the red door?"));
    assert!(scrubbed.provider_prompt.contains("Mara glances over"));
}
