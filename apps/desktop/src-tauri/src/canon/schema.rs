use crate::canon::model::CanonEntityType;
use crate::error::AppError;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PremiseValue {
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThesisValue {
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TimelineEntry {
    id: String,
    label: String,
    description: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TimelineValue {
    entries: Vec<TimelineEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AestheticValue {
    visual_register: String,
    palette: Vec<String>,
    materials: Vec<String>,
    lighting: String,
    atmosphere: String,
    exterior_presence: String,
    anomaly_rule: String,
    notes: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RelationshipsValue {
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StructuralEngine {
    id: String,
    title: String,
    description: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StructuralEnginesValue {
    engines: Vec<StructuralEngine>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActiveSkillRulesValue {
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextValue {
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VisualLock {
    id: String,
    key: String,
    description: String,
    severity: String,
    validator_hint: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VisualLocksValue {
    locks: Vec<VisualLock>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CharacterSubBeat {
    id: String,
    title: String,
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubBeatsValue {
    beats: Vec<CharacterSubBeat>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VisualTagsValue {
    tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocationRulesValue {
    rules: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductionRule {
    id: String,
    title: String,
    body: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductionRulesValue {
    rules: Vec<ProductionRule>,
}

fn trim(value: &str) -> String {
    value.trim().to_string()
}

fn trim_vec(values: Vec<String>) -> Vec<String> {
    values.into_iter().map(|value| trim(&value)).collect()
}

fn validate_unique_ids(values: &[String], field_name: &str) -> Result<(), AppError> {
    let mut seen = std::collections::HashSet::new();
    for value in values {
        if !seen.insert(value.clone()) {
            return Err(AppError::InvalidCanonSectionValue(format!(
                "{field_name} must be unique"
            )));
        }
    }
    Ok(())
}

fn validate_visual_locks(value: VisualLocksValue) -> Result<(), AppError> {
    for lock in &value.locks {
        if trim(&lock.id).is_empty() {
            return Err(AppError::InvalidCanonSectionValue(
                "Visual lock id must not be blank".to_string(),
            ));
        }
        if trim(&lock.key).is_empty() {
            return Err(AppError::InvalidCanonSectionValue(
                "Visual lock key must not be blank".to_string(),
            ));
        }
        if trim(&lock.description).is_empty() {
            return Err(AppError::InvalidCanonSectionValue(
                "Visual lock description must not be blank".to_string(),
            ));
        }
        if lock.severity != "required" && lock.severity != "important" {
            return Err(AppError::InvalidCanonSectionValue(
                "Visual lock severity must be required or important".to_string(),
            ));
        }
    }

    validate_unique_ids(
        &value.locks.iter().map(|lock| trim(&lock.key)).collect(),
        "Visual lock keys",
    )
}

fn validate_timeline(value: TimelineValue) -> Result<(), AppError> {
    for entry in &value.entries {
        if trim(&entry.id).is_empty() {
            return Err(AppError::InvalidCanonSectionValue(
                "Timeline entry id must not be blank".to_string(),
            ));
        }
    }

    validate_unique_ids(
        &value.entries.iter().map(|entry| trim(&entry.id)).collect(),
        "Timeline entry IDs",
    )
}

fn validate_structural_engines(value: StructuralEnginesValue) -> Result<(), AppError> {
    for engine in &value.engines {
        if trim(&engine.id).is_empty() {
            return Err(AppError::InvalidCanonSectionValue(
                "Structural engine id must not be blank".to_string(),
            ));
        }
    }

    validate_unique_ids(
        &value
            .engines
            .iter()
            .map(|engine| trim(&engine.id))
            .collect(),
        "Structural engine IDs",
    )
}

fn validate_sub_beats(value: SubBeatsValue) -> Result<(), AppError> {
    for beat in &value.beats {
        if trim(&beat.id).is_empty() {
            return Err(AppError::InvalidCanonSectionValue(
                "Sub-beat id must not be blank".to_string(),
            ));
        }
    }

    validate_unique_ids(
        &value.beats.iter().map(|beat| trim(&beat.id)).collect(),
        "Sub-beat IDs",
    )
}

fn validate_production_rules(value: ProductionRulesValue) -> Result<(), AppError> {
    for rule in &value.rules {
        if trim(&rule.id).is_empty() {
            return Err(AppError::InvalidCanonSectionValue(
                "Production rule id must not be blank".to_string(),
            ));
        }
    }

    validate_unique_ids(
        &value.rules.iter().map(|rule| trim(&rule.id)).collect(),
        "Production rule IDs",
    )
}

fn deserialize_and_validate<T>(value: &serde_json::Value) -> Result<T, AppError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(value.clone())
        .map_err(|_| AppError::InvalidCanonSectionValue("Payload does not match schema".to_string()))
}

fn validate_story_section(section_key: &str, value: &serde_json::Value) -> Result<(), AppError> {
    match section_key {
        "premise" => {
            let parsed: PremiseValue = deserialize_and_validate(value)?;
            let _ = trim(&parsed.text);
        }
        "thesis" => {
            let parsed: ThesisValue = deserialize_and_validate(value)?;
            let _ = trim(&parsed.text);
        }
        "timeline" => {
            let parsed: TimelineValue = deserialize_and_validate(value)?;
            validate_timeline(parsed)?;
        }
        "aesthetic" => {
            let parsed: AestheticValue = deserialize_and_validate(value)?;
            let _ = trim(&parsed.visual_register);
            let _ = trim_vec(parsed.palette);
            let _ = trim_vec(parsed.materials);
            let _ = trim(&parsed.lighting);
            let _ = trim(&parsed.atmosphere);
            let _ = trim(&parsed.exterior_presence);
            let _ = trim(&parsed.anomaly_rule);
            let _ = trim_vec(parsed.notes);
        }
        "relationships" => {
            let parsed: RelationshipsValue = deserialize_and_validate(value)?;
            let _ = trim(&parsed.text);
        }
        "structural_engines" => {
            let parsed: StructuralEnginesValue = deserialize_and_validate(value)?;
            validate_structural_engines(parsed)?;
        }
        "active_skill_rules" => {
            let parsed: ActiveSkillRulesValue = deserialize_and_validate(value)?;
            let _ = trim(&parsed.text);
        }
        _ => return Err(AppError::UnknownCanonSection),
    }

    Ok(())
}

fn validate_character_section(
    section_key: &str,
    value: &serde_json::Value,
) -> Result<(), AppError> {
    match section_key {
        "role_tag" | "visual_summary" | "function" | "backstory" | "psychology" | "speech"
        | "movement" | "stillness" => {
            let parsed: TextValue = deserialize_and_validate(value)?;
            let _ = trim(&parsed.text);
        }
        "visual_locks" => {
            let parsed: VisualLocksValue = deserialize_and_validate(value)?;
            validate_visual_locks(parsed)?;
        }
        "sub_beats" => {
            let parsed: SubBeatsValue = deserialize_and_validate(value)?;
            validate_sub_beats(parsed)?;
        }
        _ => return Err(AppError::UnknownCanonSection),
    }

    Ok(())
}

fn validate_location_section(section_key: &str, value: &serde_json::Value) -> Result<(), AppError> {
    match section_key {
        "description" | "geography" => {
            let parsed: TextValue = deserialize_and_validate(value)?;
            let _ = trim(&parsed.text);
        }
        "visual_tags" => {
            let parsed: VisualTagsValue = deserialize_and_validate(value)?;
            let _ = trim_vec(parsed.tags);
        }
        "rules" => {
            let parsed: LocationRulesValue = deserialize_and_validate(value)?;
            let _ = trim_vec(parsed.rules);
        }
        _ => return Err(AppError::UnknownCanonSection),
    }

    Ok(())
}

fn validate_faction_section(section_key: &str, value: &serde_json::Value) -> Result<(), AppError> {
    match section_key {
        "description" | "visual_signature" | "public_face" | "actual_behavior" => {
            let parsed: TextValue = deserialize_and_validate(value)?;
            let _ = trim(&parsed.text);
        }
        _ => return Err(AppError::UnknownCanonSection),
    }

    Ok(())
}

fn validate_world_rule_section(
    section_key: &str,
    value: &serde_json::Value,
) -> Result<(), AppError> {
    match section_key {
        "rule" | "notes" => {
            let parsed: TextValue = deserialize_and_validate(value)?;
            let _ = trim(&parsed.text);
        }
        _ => return Err(AppError::UnknownCanonSection),
    }

    Ok(())
}

fn validate_production_rules_section(
    section_key: &str,
    value: &serde_json::Value,
) -> Result<(), AppError> {
    match section_key {
        "rules" => {
            let parsed: ProductionRulesValue = deserialize_and_validate(value)?;
            validate_production_rules(parsed)?;
        }
        _ => return Err(AppError::UnknownCanonSection),
    }

    Ok(())
}

/// Validates a section payload against the canonical schema for the given
/// entity type and section key.
pub fn validate_section_value(
    entity_type: CanonEntityType,
    section_key: &str,
    value: &serde_json::Value,
) -> Result<(), AppError> {
    match entity_type {
        CanonEntityType::Story => validate_story_section(section_key, value),
        CanonEntityType::Character => validate_character_section(section_key, value),
        CanonEntityType::Location => validate_location_section(section_key, value),
        CanonEntityType::Faction => validate_faction_section(section_key, value),
        CanonEntityType::WorldRule => validate_world_rule_section(section_key, value),
        CanonEntityType::ProductionRules => validate_production_rules_section(section_key, value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_character_visual_locks() {
        let value = serde_json::json!({
            "locks": [{
                "id": "scar",
                "key": "right_eyebrow_scar",
                "description": "Small healed scar on character-right eyebrow.",
                "severity": "required",
                "validatorHint": "Character-right appears viewer-left in frontal images."
            }]
        });

        validate_section_value(
            CanonEntityType::Character,
            "visual_locks",
            &value,
        )
        .unwrap();
    }

    #[test]
    fn rejects_story_section_on_character() {
        let error = validate_section_value(
            CanonEntityType::Character,
            "premise",
            &serde_json::json!({"text": "x"}),
        )
        .unwrap_err();

        assert!(matches!(error, AppError::UnknownCanonSection));
    }

    #[test]
    fn rejects_invalid_visual_lock_severity() {
        let value = serde_json::json!({
            "locks": [{
                "id": "scar",
                "key": "right_eyebrow_scar",
                "description": "Scar on character-right eyebrow",
                "severity": "optional",
                "validatorHint": null
            }]
        });

        let error = validate_section_value(
            CanonEntityType::Character,
            "visual_locks",
            &value,
        )
        .unwrap_err();

        assert!(matches!(error, AppError::InvalidCanonSectionValue(_)));
    }

    #[test]
    fn rejects_duplicate_visual_lock_keys() {
        let value = serde_json::json!({
            "locks": [
                {
                    "id": "one",
                    "key": "scar",
                    "description": "A",
                    "severity": "required",
                    "validatorHint": null
                },
                {
                    "id": "two",
                    "key": "scar",
                    "description": "B",
                    "severity": "important",
                    "validatorHint": null
                }
            ]
        });

        let error = validate_section_value(
            CanonEntityType::Character,
            "visual_locks",
            &value,
        )
        .unwrap_err();

        assert!(matches!(error, AppError::InvalidCanonSectionValue(_)));
    }
}
