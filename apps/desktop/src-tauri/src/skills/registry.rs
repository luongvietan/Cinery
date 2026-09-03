use crate::error::AppError;
use crate::skills::builtin::character_builder::builtin_character_builder;
use crate::skills::builtin::scene_builder::builtin_scene_builder;
use crate::skills::builtin::video_qa::builtin_video_qa;
use crate::skills::builtin::visual_qa::builtin_visual_qa;
use crate::skills::builtin::world_builder::builtin_world_builder;
use crate::skills::model::{SkillDefinition, SkillOperation};
use crate::workflow::model::WorkflowStepDefinition;
use semver::Version;
use std::collections::HashMap;

pub struct SkillRegistry {
    skills: HashMap<String, SkillDefinition>,
}

impl SkillRegistry {
    pub fn builtin() -> Result<Self, AppError> {
        let definitions = [
            builtin_character_builder(),
            builtin_video_qa(),
            builtin_visual_qa(),
            builtin_world_builder(),
            builtin_scene_builder(),
        ];
        let mut skills = HashMap::new();
        for definition in definitions {
            validate_definition(&definition)?;
            skills.insert(
                registry_key(&definition.id, &definition.version),
                definition,
            );
        }
        Ok(Self { skills })
    }

    pub fn get(&self, skill_id: &str, version: &str) -> Result<&SkillDefinition, AppError> {
        if !self
            .skills
            .keys()
            .any(|key| key.starts_with(&format!("{skill_id}@")))
        {
            return Err(AppError::SkillNotFound(skill_id.to_string()));
        }

        self.skills
            .get(&registry_key(skill_id, version))
            .ok_or_else(|| AppError::SkillVersionNotFound(format!("{skill_id}@{version}")))
    }

    pub fn list(&self) -> Vec<&SkillDefinition> {
        let mut entries: Vec<_> = self.skills.values().collect();
        entries.sort_by(|left, right| {
            registry_key(&left.id, &left.version).cmp(&registry_key(&right.id, &right.version))
        });
        entries
    }

    /// Resolves the newest registered version of a skill by id, ignoring
    /// version. Used when a persisted lineage references a skill version
    /// that is no longer present in the registry.
    pub fn find_skill_any_version(&self, skill_id: &str) -> Option<&SkillDefinition> {
        self.skills
            .values()
            .filter(|definition| definition.id == skill_id)
            .max_by(|left, right| {
                registry_key(&left.id, &left.version).cmp(&registry_key(&right.id, &right.version))
            })
    }

    pub fn find_operation(
        &self,
        skill_id: &str,
        version: &str,
        operation_id: &str,
    ) -> Result<(&SkillDefinition, &SkillOperation), AppError> {
        let skill = self.get(skill_id, version)?;
        let operation = skill
            .operations
            .iter()
            .find(|operation| operation.id == operation_id)
            .ok_or_else(|| AppError::SkillOperationNotFound(operation_id.to_string()))?;
        Ok((skill, operation))
    }
}

fn registry_key(skill_id: &str, version: &str) -> String {
    format!("{skill_id}@{version}")
}

fn validate_identifier(value: &str, kind: &str) -> Result<(), AppError> {
    if value.is_empty()
        || !value.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || "-_.".contains(character)
        })
        || !value
            .chars()
            .any(|character| character.is_ascii_alphanumeric())
    {
        return Err(AppError::InvalidBuiltinSkillDefinition(format!(
            "{kind} has invalid identifier: {value}"
        )));
    }
    Ok(())
}

pub(crate) fn validate_definition(definition: &SkillDefinition) -> Result<(), AppError> {
    validate_identifier(&definition.id, "skill")?;
    Version::parse(&definition.version).map_err(|error| {
        AppError::InvalidBuiltinSkillDefinition(format!("invalid semver: {error}"))
    })?;

    if definition.operations.is_empty() {
        return Err(AppError::InvalidBuiltinSkillDefinition(
            "skill must define at least one operation".to_string(),
        ));
    }

    let mut operation_ids = std::collections::HashSet::new();
    for operation in &definition.operations {
        validate_identifier(&operation.id, "operation")?;
        if !operation_ids.insert(&operation.id) {
            return Err(AppError::InvalidBuiltinSkillDefinition(format!(
                "duplicate operation: {}",
                operation.id
            )));
        }
        validate_workflow(operation)?;
    }

    Ok(())
}

fn validate_workflow(operation: &SkillOperation) -> Result<(), AppError> {
    if operation.workflow.is_empty() {
        return Err(AppError::InvalidBuiltinSkillDefinition(format!(
            "operation {} must define workflow steps",
            operation.id
        )));
    }

    let mut step_ids = std::collections::HashSet::new();
    let step_types: Vec<&str> = operation
        .workflow
        .iter()
        .map(|step| match step {
            WorkflowStepDefinition::ValidateInput { .. } => "validate_input",
            WorkflowStepDefinition::ResolveContext { .. } => "resolve_context",
            WorkflowStepDefinition::CompileRequest { .. } => "compile_request",
            WorkflowStepDefinition::Approval { .. } => "approval",
            WorkflowStepDefinition::Execute { .. } => "execute",
            WorkflowStepDefinition::Complete { .. } => "complete",
        })
        .collect();
    if step_types.first() != Some(&"validate_input")
        || step_types.last() != Some(&"complete")
        || step_types
            .iter()
            .filter(|step_type| **step_type == "complete")
            .count()
            != 1
    {
        return Err(AppError::InvalidBuiltinSkillDefinition(format!(
            "operation {} must start with validate_input and end with one complete step",
            operation.id
        )));
    }

    let step_rank = |step_type: &str| match step_type {
        "validate_input" => 0,
        "resolve_context" => 1,
        "compile_request" => 2,
        "approval" => 3,
        "execute" => 4,
        "complete" => 5,
        _ => unreachable!("step type comes from the closed enum"),
    };
    if step_types
        .windows(2)
        .any(|window| step_rank(window[0]) > step_rank(window[1]))
    {
        return Err(AppError::InvalidBuiltinSkillDefinition(format!(
            "operation {} has out-of-order workflow steps",
            operation.id
        )));
    }

    if matches!(
        operation.id.as_str(),
        "character.create_face_lock"
            | "asset.run_visual_qa"
            | "asset.run_video_qa"
            | "asset.repair_failed_qa"
            | "world.create_plate"
            | "scene.create_keyframe"
            | "scene.generate_video"
            | "shot.image_to_video"
    ) && step_types
        != [
            "validate_input",
            "resolve_context",
            "compile_request",
            "approval",
            "execute",
            "complete",
        ]
    {
        return Err(AppError::InvalidBuiltinSkillDefinition(format!(
            "{} has an invalid step topology",
            operation.id
        )));
    }

    let mut approval_artifact_ref = None;
    for step in &operation.workflow {
        let id = match step {
            WorkflowStepDefinition::ValidateInput { id }
            | WorkflowStepDefinition::ResolveContext { id, .. }
            | WorkflowStepDefinition::CompileRequest { id, .. }
            | WorkflowStepDefinition::Approval { id, .. }
            | WorkflowStepDefinition::Execute { id, .. }
            | WorkflowStepDefinition::Complete { id } => id,
        };
        validate_identifier(id, "workflow step")?;
        if !step_ids.insert(id) {
            return Err(AppError::InvalidBuiltinSkillDefinition(format!(
                "duplicate workflow step: {id}"
            )));
        }

        match step {
            WorkflowStepDefinition::ResolveContext { resolver_id, .. }
                if !matches!(
                    resolver_id.as_str(),
                    "character_face_lock_context"
                        | "character_outfit_context"
                        | "character_sheet_context"
                        | "visual_qa_context"
                        | "video_qa_context"
                        | "visual_qa_repair_context"
                        | "world_plate_context"
                        | "scene_keyframe_context"
                        | "scene_video_context"
                        | "shot_image_to_video_context"
                ) =>
            {
                return Err(AppError::InvalidBuiltinSkillDefinition(format!(
                    "unknown resolver: {resolver_id}"
                )))
            }
            WorkflowStepDefinition::CompileRequest { compiler_id, .. }
                if !matches!(
                    compiler_id.as_str(),
                    "character_face_lock_v1"
                        | "character_outfit_v1"
                        | "character_sheet_v1"
                        | "visual_qa_v1"
                        | "video_qa_v1"
                        | "visual_qa_repair_v1"
                        | "world_plate_v1"
                        | "scene_keyframe_v1"
                        | "scene_video_v1"
                        | "shot_image_to_video_v1"
                ) =>
            {
                return Err(AppError::InvalidBuiltinSkillDefinition(format!(
                    "unknown compiler: {compiler_id}"
                )))
            }
            WorkflowStepDefinition::Execute { executor_kind, .. }
                if !matches!(executor_kind, crate::workflow::model::ExecutorKind::DryRun) =>
            {
                return Err(AppError::InvalidBuiltinSkillDefinition(
                    "only dry_run executor is available in P3".to_string(),
                ))
            }
            WorkflowStepDefinition::Approval {
                approval_artifact_ref: artifact_ref,
                ..
            } => {
                approval_artifact_ref = Some(artifact_ref.as_str());
            }
            WorkflowStepDefinition::Execute {
                request_artifact_ref,
                ..
            } if approval_artifact_ref.is_none()
                || approval_artifact_ref != Some(request_artifact_ref.as_str()) =>
            {
                return Err(AppError::InvalidBuiltinSkillDefinition(
                    "execute step must use the approval artifact".to_string(),
                ));
            }
            _ => {}
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::model::AssetType;

    #[test]
    fn builtin_registry_resolves_the_video_operation_with_video_expected_output() {
        let registry = SkillRegistry::builtin().unwrap();
        let (skill, operation) = registry
            .find_operation("scene-builder", "1.0.0", "scene.generate_video")
            .unwrap();
        assert_eq!(skill.id, "scene-builder");
        assert_eq!(operation.id, "scene.generate_video");
        let expected = operation.expected_output.clone().unwrap();
        assert_eq!(expected.asset_type, AssetType::Video);
        assert_eq!(
            expected.media_type,
            crate::skills::model::OutputMediaType::Video
        );
    }

    #[test]
    fn builtin_registry_resolves_the_versioned_face_lock_operation() {
        let registry = SkillRegistry::builtin().unwrap();
        let (skill, operation) = registry
            .find_operation("character-builder", "1.1.0", "character.create_face_lock")
            .unwrap();

        assert_eq!(skill.id, "character-builder");
        assert_eq!(skill.version, "1.1.0");
        assert_eq!(operation.id, "character.create_face_lock");
        assert_eq!(registry.list().len(), 5);
    }

    #[test]
    fn builtin_registry_resolves_the_outfit_and_sheet_operations() {
        let registry = SkillRegistry::builtin().unwrap();
        let (_, outfit) = registry
            .find_operation("character-builder", "1.1.0", "character.create_outfit")
            .unwrap();
        assert_eq!(outfit.input_schema_id, "create_outfit");
        assert_eq!(
            outfit.expected_output.as_ref().unwrap().asset_type,
            AssetType::Outfit
        );

        let (_, sheet) = registry
            .find_operation(
                "character-builder",
                "1.1.0",
                "character.create_character_sheet",
            )
            .unwrap();
        assert_eq!(sheet.input_schema_id, "create_character_sheet");
        assert_eq!(
            sheet.expected_output.as_ref().unwrap().asset_type,
            AssetType::CharacterSheet
        );
    }

    #[test]
    fn builtin_registry_resolves_the_versioned_visual_qa_operation() {
        let registry = SkillRegistry::builtin().unwrap();
        let (skill, operation) = registry
            .find_operation("visual-qa", "1.0.0", "asset.run_visual_qa")
            .unwrap();

        assert_eq!(skill.id, "visual-qa");
        assert_eq!(operation.input_schema_id, "run_visual_qa");
        assert!(operation.expected_output.is_none());
    }

    #[test]
    fn builtin_registry_resolves_the_versioned_video_qa_operation() {
        let registry = SkillRegistry::builtin().unwrap();
        let (skill, operation) = registry
            .find_operation("video-qa", "1.0.0", "asset.run_video_qa")
            .unwrap();

        assert_eq!(skill.id, "video-qa");
        assert_eq!(skill.version, "1.0.0");
        assert_eq!(operation.input_schema_id, "run_video_qa");
        assert!(operation.expected_output.is_none());
    }

    #[test]
    fn registry_rejects_invalid_semver_and_duplicate_step_ids() {
        let mut definition = builtin_character_builder();
        definition.version = "1.0".to_string();
        assert!(validate_definition(&definition).is_err());

        let mut definition = builtin_character_builder();
        let first = definition.operations[0].workflow[0].clone();
        definition.operations[0].workflow.push(first);
        assert!(validate_definition(&definition).is_err());
    }

    #[test]
    fn builtin_registry_resolves_the_versioned_world_plate_operation() {
        let registry = SkillRegistry::builtin().unwrap();
        let (skill, operation) = registry
            .find_operation("world-builder", "1.0.0", "world.create_plate")
            .unwrap();

        assert_eq!(skill.id, "world-builder");
        assert_eq!(skill.version, "1.0.0");
        assert_eq!(operation.id, "world.create_plate");
        assert_eq!(operation.input_schema_id, "create_world_plate");
        assert_eq!(
            operation.expected_output.as_ref().unwrap().asset_type,
            crate::skills::model::AssetType::WorldPlate
        );
        assert_eq!(registry.list().len(), 5);
        let snapshot = serde_json::to_value(skill).unwrap();
        assert!(snapshot.to_string().contains("world_plate"));
        assert!(snapshot.get("provider").is_none());
    }

    #[test]
    fn builtin_registry_resolves_the_versioned_scene_keyframe_operation() {
        let registry = SkillRegistry::builtin().unwrap();
        let (skill, operation) = registry
            .find_operation("scene-builder", "1.0.0", "scene.create_keyframe")
            .unwrap();

        assert_eq!(skill.id, "scene-builder");
        assert_eq!(skill.version, "1.0.0");
        assert_eq!(operation.id, "scene.create_keyframe");
        assert_eq!(operation.input_schema_id, "create_scene_keyframe");
        assert_eq!(
            operation.expected_output.as_ref().unwrap().asset_type,
            crate::skills::model::AssetType::ShotKeyframe
        );
        assert_eq!(registry.list().len(), 5);
        let snapshot = serde_json::to_value(skill).unwrap();
        assert!(snapshot.to_string().contains("shot_keyframe"));
        assert!(snapshot.get("provider").is_none());
    }

    #[test]
    fn registry_rejects_invalid_face_lock_step_topology_and_identifiers() {
        let mut definition = builtin_character_builder();
        definition.operations[0].workflow.swap(0, 1);
        assert!(validate_definition(&definition).is_err());

        let mut definition = builtin_character_builder();
        definition.id = "---".to_string();
        assert!(validate_definition(&definition).is_err());

        let mut definition = builtin_character_builder();
        definition.operations[0].id = "other.operation".to_string();
        definition.operations[0].workflow.pop();
        assert!(validate_definition(&definition).is_err());

        let mut definition = builtin_character_builder();
        definition.operations[0].id = "other.operation".to_string();
        definition.operations[0].workflow.swap(0, 1);
        assert!(validate_definition(&definition).is_err());

        let mut definition = builtin_character_builder();
        definition.operations[0].id = "other.operation".to_string();
        definition.operations[0].workflow.remove(3);
        assert!(validate_definition(&definition).is_err());
    }

    #[test]
    fn serialized_builtin_definition_is_stable_and_provider_free() {
        let registry = SkillRegistry::builtin().unwrap();
        let skill = registry
            .list()
            .iter()
            .find(|skill| skill.id == "character-builder")
            .unwrap()
            .to_owned();
        let snapshot = serde_json::to_value(skill).unwrap();
        let operations = snapshot["operations"].as_array().unwrap();

        assert_eq!(snapshot["id"], "character-builder");
        assert_eq!(snapshot["version"], "1.1.0");
        assert_eq!(snapshot.get("provider"), None);
        assert_eq!(snapshot.get("model"), None);
        assert_eq!(operations.len(), 3);
        assert_eq!(operations[0]["id"], "character.create_face_lock");
        assert_eq!(operations[1]["id"], "character.create_outfit");
        assert_eq!(operations[2]["id"], "character.create_character_sheet");
        assert_eq!(
            operations[1]["prerequisites"][1],
            serde_json::json!({
                "type": "canonical_asset_exists",
                "ownerEntityInputRef": "characterEntityId",
                "assetType": "face_lock"
            })
        );
        assert_eq!(
            operations[2]["prerequisites"][1],
            serde_json::json!({
                "type": "canonical_asset_exists",
                "ownerEntityInputRef": "characterEntityId",
                "assetType": "outfit"
            })
        );
        assert!(operations[0]["intentExamples"].is_array());
        assert_eq!(
            operations[0]["expectedOutput"],
            serde_json::json!({
                "assetType": "face_lock",
                "mediaType": "image",
                "desiredStatus": "candidate",
                "ownerEntityInputRef": "characterEntityId"
            })
        );
    }
}
