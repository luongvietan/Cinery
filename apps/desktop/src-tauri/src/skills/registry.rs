use crate::error::AppError;
use crate::skills::builtin::character_builder::builtin_character_builder;
use crate::skills::model::{SkillDefinition, SkillOperation};
use crate::workflow::model::WorkflowStepDefinition;
use semver::Version;
use std::collections::HashMap;

pub struct SkillRegistry {
    skills: HashMap<String, SkillDefinition>,
}

impl SkillRegistry {
    pub fn builtin() -> Result<Self, AppError> {
        let definition = builtin_character_builder();
        validate_definition(&definition)?;

        let key = registry_key(&definition.id, &definition.version);
        Ok(Self {
            skills: HashMap::from([(key, definition)]),
        })
    }

    pub fn get(&self, skill_id: &str, version: &str) -> Result<&SkillDefinition, AppError> {
        if !self.skills.keys().any(|key| key.starts_with(&format!("{skill_id}@"))) {
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
        || !value
            .chars()
            .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit() || "-_.".contains(character))
        || !value.chars().any(|character| character.is_ascii_alphanumeric())
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
        || step_types.iter().filter(|step_type| **step_type == "complete").count() != 1
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

    if operation.id == "character.create_face_lock"
        && step_types
            != [
                "validate_input",
                "resolve_context",
                "compile_request",
                "approval",
                "execute",
                "complete",
            ]
    {
        return Err(AppError::InvalidBuiltinSkillDefinition(
            "character.create_face_lock has an invalid step topology".to_string(),
        ));
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
                if resolver_id != "character_face_lock_context" =>
            {
                return Err(AppError::InvalidBuiltinSkillDefinition(format!(
                    "unknown resolver: {resolver_id}"
                )))
            }
            WorkflowStepDefinition::CompileRequest { compiler_id, .. }
                if compiler_id != "character_face_lock_v1" =>
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
                || approval_artifact_ref != Some(request_artifact_ref.as_str()) => {
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

    #[test]
    fn builtin_registry_resolves_the_versioned_face_lock_operation() {
        let registry = SkillRegistry::builtin().unwrap();
        let (skill, operation) = registry
            .find_operation(
                "character-builder",
                "1.0.0",
                "character.create_face_lock",
            )
            .unwrap();

        assert_eq!(skill.id, "character-builder");
        assert_eq!(skill.version, "1.0.0");
        assert_eq!(operation.id, "character.create_face_lock");
        assert_eq!(registry.list().len(), 1);
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
        let snapshot = serde_json::to_value(registry.list()[0]).unwrap();

        assert_eq!(snapshot, serde_json::json!({
            "id": "character-builder",
            "name": "Character Builder",
            "version": "1.0.0",
            "description": "Build character production assets from locked Canon.",
            "operations": [{
                "id": "character.create_face_lock",
                "name": "Create Face Lock",
                "description": "Compile a provider-neutral face-lock request.",
                "intentExamples": [
                    "Create a face lock for this character",
                    "Lock the character's face"
                ],
                "inputSchemaId": "create_face_lock",
                "prerequisites": [{
                    "type": "canon_entity_exists",
                    "entityType": "character",
                    "inputRef": "characterEntityId"
                }],
                "tbdGuards": [],
                "workflow": [
                    {"type": "validate_input", "id": "validate-input"},
                    {"type": "resolve_context", "id": "resolve-context", "resolverId": "character_face_lock_context"},
                    {"type": "compile_request", "id": "compile-request", "compilerId": "character_face_lock_v1"},
                    {"type": "approval", "id": "approve-request", "title": "Approve Face Lock Request", "description": "Review canonical context and compiled generation request before execution.", "approvalArtifactRef": "compiled_request"},
                    {"type": "execute", "id": "execute", "executorKind": "dry_run", "requestArtifactRef": "compiled_request"},
                    {"type": "complete", "id": "complete"}
                ],
                "expectedOutput": {
                    "assetType": "face_lock",
                    "mediaType": "image",
                    "desiredStatus": "candidate",
                    "ownerEntityInputRef": "characterEntityId"
                }
            }]
        }));

        assert!(snapshot.get("provider").is_none());
        assert!(snapshot.get("model").is_none());
    }
}
