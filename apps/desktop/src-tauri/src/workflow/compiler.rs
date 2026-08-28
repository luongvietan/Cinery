use crate::error::AppError;
use crate::skills::model::{SkillDefinition, SkillOperation};
use crate::workflow::execution::{
    ExecutionConstraint, ExecutionMediaType, ExecutionProvenance, ExecutionReference,
    ExecutionReferenceType, ExecutionRequest, ExecutionTask, ReferenceBackground,
};
use crate::workflow::model::WorkflowContextSnapshot;

pub trait RequestCompiler {
    fn id(&self) -> &'static str;
    fn compile(
        &self,
        workflow_run_id: &str,
        skill: &SkillDefinition,
        operation: &SkillOperation,
        context: &WorkflowContextSnapshot,
    ) -> Result<ExecutionRequest, AppError>;
}

pub struct CharacterFaceLockCompiler;

impl RequestCompiler for CharacterFaceLockCompiler {
    fn id(&self) -> &'static str {
        "character_face_lock_v1"
    }

    fn compile(
        &self,
        workflow_run_id: &str,
        skill: &SkillDefinition,
        operation: &SkillOperation,
        context: &WorkflowContextSnapshot,
    ) -> Result<ExecutionRequest, AppError> {
        let character = required_context(&context.resolved_context, "character")?;
        let visual_spec = required_context(&context.resolved_context, "detailedVisualSpec")?;
        let wardrobe = context
            .resolved_context
            .get("baselineWardrobe")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                AppError::WorkflowRunInconsistent("baselineWardrobe is missing from context".into())
            })?;
        let locks = character
            .get("permanentVisualLocks")
            .cloned()
            .ok_or_else(|| {
                AppError::WorkflowRunInconsistent(
                    "permanentVisualLocks is missing from context".into(),
                )
            })?;
        let expression = visual_spec
            .get("expression")
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                AppError::WorkflowRunInconsistent(
                    "visualSpec.expression is missing from context".into(),
                )
            })?;
        let mut prompt = String::new();
        prompt.push_str("TASK\nCreate a neutral character face-lock reference plate.\n\n");
        prompt.push_str("VISUAL SPEC\n");
        prompt.push_str(&serde_json::to_string_pretty(&visual_spec).map_err(db_error)?);
        prompt.push_str("\n\nLOCKED BIBLE-LEVEL VISUAL CONTEXT\n");
        prompt.push_str(
            &serde_json::to_string_pretty(&json_without_story_name(&character))
                .map_err(db_error)?,
        );
        prompt.push_str("\n\nPERMANENT VISUAL LOCKS\n");
        prompt.push_str(&serde_json::to_string_pretty(&locks).map_err(db_error)?);
        prompt.push_str("\n\nBASELINE WARDROBE\n");
        prompt.push_str(wardrobe);
        prompt.push_str("\n\nPOSE / EXPRESSION\nFront-facing neutral reference pose; expression: ");
        prompt.push_str(expression);
        prompt.push_str("\n\nREFERENCE PLATE RULES\nflat 18% neutral gray field; flat shadowless neutral illumination; no cast shadow; no contact shadow; no cinematic depth of field; biological realism.\n\nFORBIDDEN STYLIZATION\nNo stylized rendering, beauty lighting, or cinematic depth of field.\n\nOUTPUT INTENT\nface_lock image candidate.\n");
        prompt = prompt.replace("\n\nFORBIDDEN STYLIZATION", "\n\nBIOLOGICAL REALISM\nNatural anatomy, skin texture, and physically plausible proportions.\n\nFORBIDDEN STYLIZATION");

        let mut references = context
            .canon
            .iter()
            .map(|reference| ExecutionReference {
                reference_type: ExecutionReferenceType::CanonSnapshot,
                reference: reference.section_id.clone(),
                description: format!(
                    "Locked Canon section {} at revision {}",
                    reference.section_key, reference.revision
                ),
            })
            .collect::<Vec<_>>();
        references.extend(context.assets.iter().map(|reference| ExecutionReference {
            reference_type: ExecutionReferenceType::AssetVersion,
            reference: reference.asset_version_id.clone(),
            description: format!(
                "Canonical {} asset version {}",
                reference.asset_type.as_str(),
                reference.version_number
            ),
        }));
        let mut constraints = vec![
            ExecutionConstraint::FlatReferenceBackground {
                value: ReferenceBackground::NeutralGray,
            },
            ExecutionConstraint::ShadowlessLighting { value: true },
            ExecutionConstraint::NoCastShadow { value: true },
            ExecutionConstraint::NoContactShadow { value: true },
            ExecutionConstraint::NoCinematicDof { value: true },
        ];
        if let Some(lock_values) = locks.as_array() {
            constraints.extend(lock_values.iter().filter_map(|lock| {
                Some(ExecutionConstraint::PreserveVisualLock {
                    key: lock.get("key")?.as_str()?.to_string(),
                    description: lock.get("description")?.as_str()?.to_string(),
                })
            }));
        }

        Ok(ExecutionRequest {
            request_version: 1,
            task: ExecutionTask::CharacterFaceLock,
            media_type: ExecutionMediaType::Image,
            prompt,
            references,
            constraints,
            expected_output: operation.expected_output.clone().ok_or_else(|| {
                AppError::WorkflowRunInconsistent(
                    "face-lock operation has no expected output".into(),
                )
            })?,
            provenance: ExecutionProvenance {
                workflow_run_id: workflow_run_id.into(),
                skill_id: skill.id.clone(),
                skill_version: skill.version.clone(),
                operation_id: operation.id.clone(),
            },
        })
    }
}

fn json_without_story_name(character: &serde_json::Value) -> serde_json::Value {
    let mut value = character.clone();
    if let Some(object) = value.as_object_mut() {
        object.remove("storyName");
    }
    value
}

fn required_context(context: &serde_json::Value, key: &str) -> Result<serde_json::Value, AppError> {
    context
        .get(key)
        .cloned()
        .ok_or_else(|| AppError::WorkflowRunInconsistent(format!("{key} is missing from context")))
}

fn db_error(error: serde_json::Error) -> AppError {
    AppError::WorkflowRunInconsistent(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::registry::SkillRegistry;

    #[test]
    fn face_lock_compilation_is_deterministic_and_omits_story_name() {
        let registry = SkillRegistry::builtin().unwrap();
        let (skill, operation) = registry
            .find_operation("character-builder", "1.0.0", "character.create_face_lock")
            .unwrap();
        let context: WorkflowContextSnapshot = serde_json::from_value(serde_json::json!({
            "snapshotVersion": 1,
            "project": { "projectId": "project-1" },
            "skill": { "skillId": "character-builder", "skillVersion": "1.0.0", "operationId": "character.create_face_lock" },
            "input": {},
            "prerequisiteReport": { "passed": true, "checks": [] },
            "canon": [],
            "assets": [],
            "protectedTbds": [],
            "resolvedContext": {
                "character": { "entityId": "mara", "storyName": "Mara", "roleTag": "Protagonist", "visualSummary": "Angular face.", "permanentVisualLocks": [] },
                "detailedVisualSpec": { "eyes": "brown", "expression": "neutral" },
                "baselineWardrobe": "charcoal crew neck",
                "referencePlateRules": {}
            },
            "capturedAt": "2026-08-28T00:00:00Z"
        })).unwrap();

        let first = CharacterFaceLockCompiler
            .compile("run-1", skill, operation, &context)
            .unwrap();
        let second = CharacterFaceLockCompiler
            .compile("run-1", skill, operation, &context)
            .unwrap();

        assert_eq!(
            serde_json::to_vec(&first).unwrap(),
            serde_json::to_vec(&second).unwrap()
        );
        assert!(!first.prompt.contains("Mara"));
        assert!(first.prompt.contains("Angular face."));
    }
}
