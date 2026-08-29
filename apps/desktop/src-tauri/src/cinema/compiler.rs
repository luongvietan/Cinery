use crate::canon::service::VisualLockDto;
use crate::cinema::model::{
    compute_time_budget, BehavioralLocks, ProviderNeutralCinemaPrompt, SceneRef, ShotInstruction,
    ShotRecord, SubjectLock, WorldContinuity,
};
use crate::error::AppError;

/// Compiles the provider-neutral cinema prompt for one scene.
///
/// Deterministic by construction: shots are ordered by index, visual locks
/// are sorted by key, and no timestamp enters the output (the compilation
/// id is supplied by the caller; `createdAt` lives on the persisted record,
/// not in the prompt). `forbidden_topics` carries the topics of open TBDs —
/// any occurrence in user-authored shot text is scrubbed so unresolved
/// story questions never leak into generation prompts (master plan #11).
#[allow(clippy::too_many_arguments)]
pub fn compile(
    scene: &SceneRef,
    compilation_id: &str,
    total_duration_seconds: f64,
    _shot_count: Option<usize>,
    behavioral_locks: &BehavioralLocks,
    world_continuity: &WorldContinuity,
    visual_locks: &[VisualLockDto],
    shots: &[ShotRecord],
    forbidden_topics: &[String],
) -> Result<ProviderNeutralCinemaPrompt, AppError> {
    if shots.is_empty() {
        return Err(AppError::WorkflowPrerequisiteFailed(
            "scene has no shots".into(),
        ));
    }

    let time_budget = compute_time_budget(total_duration_seconds, Some(shots.len()))?;

    // Deterministic visual locks: sorted by key, de-duplicated.
    let mut subject_locks: Vec<SubjectLock> = visual_locks
        .iter()
        .map(|lock| SubjectLock {
            id: lock.id.clone(),
            key: lock.key.clone(),
            description: lock.description.clone(),
        })
        .collect();
    subject_locks.sort_by(|a, b| a.key.cmp(&b.key).then(a.id.cmp(&b.id)));
    subject_locks.dedup_by(|a, b| a.id == b.id && a.key == b.key);

    let look_ref = "canonical look".to_string();
    let world_ref = world_continuity
        .plate_asset_version_id
        .clone()
        .or_else(|| world_continuity.plate_id.clone())
        .unwrap_or_else(|| "world baseline".to_string());
    let continuity_note = format!(
        "Preserve canonical look {look_ref} and world plate {world_ref} across shots; \
         character placement consistent with geography"
    );

    let shot_instructions: Vec<ShotInstruction> = shots
        .iter()
        .enumerate()
        .map(|(index, shot)| ShotInstruction {
            order: index,
            duration_seconds: time_budget[index],
            intent: scrub(&shot.intent, forbidden_topics),
            action: shot
                .action
                .as_deref()
                .map(|action| scrub(action, forbidden_topics)),
            camera: shot.camera.clone(),
            continuity_note: Some(continuity_note.clone()),
            subject_locks: subject_locks.clone(),
        })
        .collect();
    build_prompt(
        scene,
        compilation_id,
        total_duration_seconds,
        behavioral_locks,
        world_continuity,
        subject_locks,
        world_ref,
        look_ref,
        shot_instructions,
        time_budget,
        forbidden_topics,
    )
}

/// Renders durations without trailing zeroes (8 -> "8", 3.34 -> "3.34").
fn format_number(value: f64) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    if (rounded - rounded.trunc()).abs() < f64::EPSILON {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded}")
    }
}

/// Removes every case-insensitive occurrence of each forbidden topic from
/// `text`, then collapses leftover whitespace.
fn scrub(text: &str, forbidden_topics: &[String]) -> String {
    let mut result = text.to_string();
    for topic in forbidden_topics {
        if topic.is_empty() {
            continue;
        }
        loop {
            let lowered = result.to_lowercase();
            let needle = topic.to_lowercase();
            match lowered.find(&needle) {
                Some(start) => {
                    let end = (start + topic.len()).min(result.len());
                    result.replace_range(start..end, "");
                }
                None => break,
            }
        }
    }
    result.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Assembles the deterministic prompt text and the structured prompt value.
#[allow(clippy::too_many_arguments)]
fn build_prompt(
    scene: &SceneRef,
    compilation_id: &str,
    total_duration_seconds: f64,
    behavioral_locks: &BehavioralLocks,
    world_continuity: &WorldContinuity,
    subject_locks: Vec<SubjectLock>,
    world_ref: String,
    _look_ref: String,
    shot_instructions: Vec<ShotInstruction>,
    time_budget: Vec<f64>,
    forbidden_topics: &[String],
) -> Result<ProviderNeutralCinemaPrompt, AppError> {
    let time_budget_json =
        serde_json::to_string(&time_budget).map_err(|e| AppError::Database(e.to_string()))?;
    let visual_locks_json =
        serde_json::to_string(&subject_locks).map_err(|e| AppError::Database(e.to_string()))?;

    let world_description = world_continuity
        .description
        .clone()
        .unwrap_or_else(|| "unspecified world baseline".to_string());
    let audio_instructions = if scene.summary.trim().is_empty() {
        None
    } else {
        Some(scrub(&scene.summary, forbidden_topics))
    };

    let last_frame = shot_instructions
        .last()
        .map(|shot| {
            format!(
                "{} — {} — {}",
                shot.camera.as_deref().unwrap_or("unspecified camera"),
                shot.intent,
                world_description
            )
        })
        .unwrap_or_default();

    let mut prompt = String::new();
    prompt.push_str("CINEMA PRODUCTION PROMPT — Provider Neutral\n");
    prompt.push_str(&format!(
        "Project: {} Scene: {} ({})\n",
        scene.project_id, scene.title, scene.id
    ));
    prompt.push_str(&format!(
        "Runtime: {}s across {} shots\n",
        format_number(total_duration_seconds),
        shot_instructions.len()
    ));
    prompt.push_str(&format!("Time Budget: {time_budget_json}\n"));
    prompt.push_str("Character Behavioral Locks:\n");
    prompt.push_str(&format!(
        "  speech: {}\n  movement: {}\n  stillness: {}\n",
        behavioral_locks.speech.as_deref().unwrap_or(""),
        behavioral_locks.movement.as_deref().unwrap_or(""),
        behavioral_locks.stillness.as_deref().unwrap_or(""),
    ));
    prompt.push_str(&format!("Visual Locks: {visual_locks_json}\n"));
    prompt.push_str(&format!(
        "World Continuity: plate {} — preserve architecture/materials/lighting baseline. {}\n",
        world_ref, world_description
    ));
    prompt.push_str("Shots:\n");
    for (index, shot) in shot_instructions.iter().enumerate() {
        prompt.push_str(&format!(
            "  {}. [{}s] {} — camera: {} action: {} continuity: {}\n",
            index + 1,
            format_number(shot.duration_seconds),
            shot.intent,
            shot.camera.as_deref().unwrap_or("unspecified"),
            shot.action.as_deref().unwrap_or("unspecified"),
            shot.continuity_note.as_deref().unwrap_or(""),
        ));
    }
    prompt.push_str(
        "Continuity: each shot preserves canonical look and world; no lens over-lock; \
         do not invent beyond protected or unresolved TBD topics\n",
    );
    prompt.push_str(&format!(
        "Audio: {}\n",
        audio_instructions.as_deref().unwrap_or("none")
    ));
    prompt.push_str(&format!("Last Frame: {last_frame}\n"));
    prompt.push_str(&format!(
        "Provenance: story bible + scene {} + compilation {compilation_id}\n",
        scene.id
    ));

    Ok(ProviderNeutralCinemaPrompt {
        project_id: scene.project_id.clone(),
        scene_id: scene.id.clone(),
        compilation_id: compilation_id.to_string(),
        total_duration_seconds,
        time_budget,
        shots: shot_instructions,
        behavioral_locks: behavioral_locks.clone(),
        world_continuity: world_continuity.clone(),
        continuity: "each shot preserves canonical look and world; no lens over-lock".into(),
        audio_instructions,
        last_frame: Some(last_frame),
        provider_prompt: prompt,
    })
}
