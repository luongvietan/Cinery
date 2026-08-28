use crate::error::AppError;
use serde::{Deserialize, Serialize};

/// A persisted scene: one narrative unit inside a project, optionally
/// anchored to a canonical world plate version and free-form canon notes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneRecord {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub world_asset_version_id: Option<String>,
    pub canon_notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One character cast into a scene, pinned to a canonical look version
/// (outfit / character sheet) and optionally a canonical sheet version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneCharacterRecord {
    pub scene_id: String,
    pub character_entity_id: String,
    pub look_asset_version_id: String,
    pub sheet_asset_version_id: Option<String>,
    pub display_order: i64,
}

/// One prop pinned into a scene via a canonical prop plate version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScenePropRecord {
    pub scene_id: String,
    pub prop_asset_version_id: String,
    pub display_order: i64,
}

/// A persisted shot belonging to a scene. Ordering is unique per scene and
/// durations are bounded by the schema (0 < duration <= 30s).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShotRecord {
    pub id: String,
    pub scene_id: String,
    pub ordering: i64,
    pub duration_seconds: f64,
    pub keyframe_asset_version_id: Option<String>,
    pub intent: String,
    pub action: Option<String>,
    pub camera: Option<String>,
    pub generated_video_asset_version_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Caller-supplied input for one compilation run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CinemaCompileInput {
    pub scene_id: String,
    pub total_duration_seconds: f64,
    /// When omitted, the compiler auto-sizes the shot list (~4s per shot).
    pub shot_count: Option<usize>,
}

/// Locked behavioral canon for one character, interpolated into every
/// compiled prompt.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BehavioralLocks {
    pub speech: Option<String>,
    pub movement: Option<String>,
    pub stillness: Option<String>,
}

/// World continuity constraints resolved from the scene's canonical world
/// plate (and, when present, its location entity description).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorldContinuity {
    pub plate_id: Option<String>,
    pub plate_asset_version_id: Option<String>,
    pub description: Option<String>,
}

/// A single visual lock (e.g. scar, watch) carried into shot instructions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubjectLock {
    pub id: String,
    pub key: String,
    pub description: String,
}

/// One per-shot instruction inside the compiled provider-neutral prompt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShotInstruction {
    pub order: usize,
    pub duration_seconds: f64,
    pub intent: String,
    pub action: Option<String>,
    pub camera: Option<String>,
    pub continuity_note: Option<String>,
    pub subject_locks: Vec<SubjectLock>,
}

/// The compiled, provider-neutral cinema prompt. Must never contain
/// `providerId`/`modelId`-style fields -- see `compilation_has_no_provider_fields`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderNeutralCinemaPrompt {
    pub project_id: String,
    pub scene_id: String,
    pub compilation_id: String,
    pub total_duration_seconds: f64,
    pub time_budget: Vec<f64>,
    pub shots: Vec<ShotInstruction>,
    pub behavioral_locks: BehavioralLocks,
    pub world_continuity: WorldContinuity,
    pub continuity: String,
    pub audio_instructions: Option<String>,
    pub last_frame: Option<String>,
    pub provider_prompt: String,
}

/// The persisted compilation record: input snapshot, compiled JSON, and the
/// export artifact location plus its content hash.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CinemaCompilation {
    pub id: String,
    pub project_id: String,
    pub scene_id: String,
    pub input_json: String,
    pub compilation_json: String,
    pub export_path: String,
    pub export_sha256: String,
    pub created_at: String,
}

/// Validates a total compilation runtime (whole scene): bounded to 1-120s
/// per the P8 master plan.
pub fn validate_total_duration(seconds: f64) -> Result<f64, AppError> {
    if !seconds.is_finite() || !(1.0..=120.0).contains(&seconds) {
        return Err(AppError::InvalidCinemaDuration(
            "total duration must be between 1 and 120 seconds".into(),
        ));
    }
    Ok(seconds)
}

/// Validates one shot's duration: bounded to 0.5-30s (schema caps at 30).
pub fn validate_shot_duration(seconds: f64) -> Result<f64, AppError> {
    if !seconds.is_finite() || !(0.5..=30.0).contains(&seconds) {
        return Err(AppError::InvalidCinemaDuration(
            "each shot must be between 0.5 and 30 seconds".into(),
        ));
    }
    Ok(seconds)
}

/// Splits `total_seconds` across `shot_count` shots. When `shot_count` is
/// `None` the budget is auto-sized at ~4s per shot (minimum one). Deterministic:
/// remainder centiseconds go to earlier shots and the parts always sum
/// exactly to the validated total.
pub fn compute_time_budget(
    total_seconds: f64,
    shot_count: Option<usize>,
) -> Result<Vec<f64>, AppError> {
    validate_total_duration(total_seconds)?;
    let count = match shot_count {
        Some(count) if count > 0 => count,
        Some(count) => {
            return Err(AppError::InvalidCinemaDuration(format!(
                "shot count must be positive, got {count}"
            )))
        }
        None => ((total_seconds / 4.0).ceil() as usize).max(1),
    };
    let total_cs = (total_seconds * 100.0).round() as i64;
    let base = total_cs / count as i64;
    let remainder = total_cs % count as i64;
    Ok((0..count)
        .map(|index| {
            let cs = base + if (index as i64) < remainder { 1 } else { 0 };
            cs as f64 / 100.0
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compilation_has_no_provider_fields() {
        let prompt = ProviderNeutralCinemaPrompt {
            project_id: "p".into(),
            scene_id: "s".into(),
            compilation_id: "c".into(),
            total_duration_seconds: 8.0,
            time_budget: vec![4.0, 4.0],
            shots: vec![ShotInstruction {
                order: 0,
                duration_seconds: 4.0,
                intent: "Establish".into(),
                action: Some("stand".into()),
                camera: Some("wide".into()),
                continuity_note: Some("keep look".into()),
                subject_locks: vec![SubjectLock {
                    id: "scar".into(),
                    key: "right_eyebrow_scar".into(),
                    description: "Small healed scar.".into(),
                }],
            }],
            behavioral_locks: BehavioralLocks {
                speech: Some("calm".into()),
                movement: Some("precise".into()),
                stillness: Some("restrained".into()),
            },
            world_continuity: WorldContinuity {
                plate_id: Some("wp".into()),
                plate_asset_version_id: Some("wp-v1".into()),
                description: Some("Station".into()),
            },
            continuity: "each shot preserves canonical look".into(),
            audio_instructions: None,
            last_frame: None,
            provider_prompt: "CINEMA PROMPT".into(),
        };

        let json = serde_json::to_value(&prompt).unwrap();
        fn walk(value: &serde_json::Value, keys: &mut Vec<String>) {
            match value {
                serde_json::Value::Object(map) => {
                    for (key, nested) in map {
                        keys.push(key.clone());
                        walk(nested, keys);
                    }
                }
                serde_json::Value::Array(items) => {
                    for item in items {
                        walk(item, keys);
                    }
                }
                _ => {}
            }
        }
        let mut keys = Vec::new();
        walk(&json, &mut keys);
        assert!(!keys.is_empty());
        assert!(
            !keys.iter().any(|key| key == "providerId" || key == "modelId"),
            "provider-neutral prompt must not leak provider/model identifiers"
        );
        assert_eq!(json["behavioralLocks"]["speech"], "calm");
        assert_eq!(json["totalDurationSeconds"], 8.0);
    }

    #[test]
    fn validates_total_and_shot_durations() {
        assert!(validate_total_duration(0.9).is_err());
        assert!(validate_total_duration(120.1).is_err());
        assert!(validate_total_duration(8.0).is_ok());

        assert!(validate_shot_duration(0.4).is_err());
        assert!(validate_shot_duration(30.0).is_ok());
        assert!(validate_shot_duration(30.5).is_err());
        assert!(validate_shot_duration(f64::NAN).is_err());
    }

    #[test]
    fn time_budget_auto_sizes_and_sums_exactly() {
        let budget = compute_time_budget(8.0, None).unwrap();
        assert_eq!(budget, vec![4.0, 4.0]);

        let budget = compute_time_budget(10.0, Some(3)).unwrap();
        assert_eq!(budget, vec![3.34, 3.33, 3.33]);
        let sum: f64 = budget.iter().sum();
        assert!((sum - 10.0).abs() < 1e-9);

        assert!(compute_time_budget(8.0, Some(0)).is_err());
    }
}
