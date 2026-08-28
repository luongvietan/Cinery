use crate::error::AppError;
use crate::skills::model::{SkillDefinition, SkillOperation};
use crate::workflow::execution::{
    ExecutionConstraint, ExecutionMediaType, ExecutionProvenance, ExecutionReference,
    ExecutionReferenceType, ExecutionRequest, ExecutionTask, ReferenceBackground,
};
use crate::workflow::model::WorkflowContextSnapshot;
use serde_json::Value;

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

pub struct WorldPlateCompiler;

impl RequestCompiler for WorldPlateCompiler {
    fn id(&self) -> &'static str {
        "world_plate_v1"
    }

    fn compile(
        &self,
        workflow_run_id: &str,
        skill: &SkillDefinition,
        operation: &SkillOperation,
        context: &WorkflowContextSnapshot,
    ) -> Result<ExecutionRequest, AppError> {
        let world = required_context(&context.resolved_context, "world")?;
        let location = required_context(&context.resolved_context, "location")?;
        let description = location
            .get("description")
            .and_then(|v| v.as_str())
            .filter(|v| !v.trim().is_empty())
            .ok_or_else(|| {
                AppError::WorkflowRunInconsistent("location.description is missing from context".into())
            })?;
        let geography = location
            .get("geography")
            .and_then(|v| v.as_str())
            .filter(|v| !v.trim().is_empty())
            .ok_or_else(|| {
                AppError::WorkflowRunInconsistent("location.geography is missing from context".into())
            })?;
        let visual_tags = location.get("visualTags").cloned().unwrap_or(Value::Null);
        let location_rules = location.get("rules").cloned().unwrap_or(Value::Null);
        let aesthetic = context.resolved_context.get("aesthetic").cloned().unwrap_or(Value::Null);
        let world_rules = context
            .resolved_context
            .get("worldRules")
            .cloned()
            .unwrap_or(Value::Array(Vec::new()));
        let production_rules = context
            .resolved_context
            .get("productionRules")
            .cloned()
            .unwrap_or(Value::Array(Vec::new()));
        let tbd_decisions = context
            .resolved_context
            .get("tbdDecisions")
            .cloned()
            .unwrap_or(Value::Array(Vec::new()));

        let mut prompt = String::new();
        prompt.push_str("TASK\nCreate a persistent environment reference plate.\n\n");
        prompt.push_str("ENVIRONMENT TRUTH\nTreat architecture, geography, materials, recurring set dressing and permanent environmental features as canonical environment truth.\n\n");
        prompt.push_str("CHARACTER POLICY\nDo not introduce characters unless explicitly required by a later Scene workflow.\n\n");
        prompt.push_str("OVER-LOCK AVOIDANCE\nDo not over-lock: specific lens, fixed composition, exact character placement, shot-specific blocking.\nThe image represents the place, not one particular shot inside the place.\n\n");
        prompt.push_str("LOCATION DESCRIPTION\n");
        prompt.push_str(description);
        prompt.push_str("\n\nLOCATION GEOGRAPHY\n");
        prompt.push_str(geography);
        if let Some(tags) = visual_tags.as_array() {
            if !tags.is_empty() {
                prompt.push_str("\n\nVISUAL TAGS\n");
                prompt.push_str(&serde_json::to_string_pretty(&visual_tags).map_err(db_error)?);
            }
        }
        if let Some(rules) = location_rules.as_array() {
            if !rules.is_empty() {
                prompt.push_str("\n\nLOCATION RULES\n");
                prompt.push_str(&serde_json::to_string_pretty(&location_rules).map_err(db_error)?);
            }
        }
        if aesthetic != Value::Null {
            prompt.push_str("\n\nAESTHETIC\n");
            prompt.push_str(&serde_json::to_string_pretty(&aesthetic).map_err(db_error)?);
        }
        if let Some(rules) = world_rules.as_array() {
            if !rules.is_empty() {
                prompt.push_str("\n\nWORLD RULES\n");
                prompt.push_str(&serde_json::to_string_pretty(&world_rules).map_err(db_error)?);
            }
        }
        if let Some(rules) = production_rules.as_array() {
            if !rules.is_empty() {
                prompt.push_str("\n\nPRODUCTION RULES\n");
                prompt.push_str(&serde_json::to_string_pretty(&production_rules).map_err(db_error)?);
            }
        }
        if let Some(decisions) = tbd_decisions.as_array() {
            if !decisions.is_empty() {
                prompt.push_str("\n\nPROTECTED TBD CONSTRAINTS\n");
                prompt.push_str(&serde_json::to_string_pretty(&tbd_decisions).map_err(db_error)?);
            }
        }
        prompt.push_str("\n\nWORLD REFERENCE\n");
        prompt.push_str(&serde_json::to_string_pretty(&world).map_err(db_error)?);
        prompt.push_str("\n\nCANONICAL CONSTRAINT\nDo not attach irrelevant Character canon.\n");
        prompt.push_str("\nOUTPUT INTENT\nworld_plate image candidate.\n");

        let references = context
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

        let constraints = Vec::new();

        Ok(ExecutionRequest {
            request_version: 1,
            task: ExecutionTask::WorldPlate,
            media_type: ExecutionMediaType::Image,
            prompt,
            references,
            constraints,
            expected_output: operation.expected_output.clone().ok_or_else(|| {
                AppError::WorkflowRunInconsistent(
                    "world-plate operation has no expected output".into(),
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

    #[test]
    fn world_plate_compilation_is_deterministic_and_contains_required_prompt_phrases() {
        let registry = SkillRegistry::builtin().unwrap();
        let (skill, operation) = registry
            .find_operation("world-builder", "1.0.0", "world.create_plate")
            .unwrap();
        let context: WorkflowContextSnapshot = serde_json::from_value(serde_json::json!({
            "snapshotVersion": 1,
            "project": { "projectId": "project-1" },
            "skill": { "skillId": "world-builder", "skillVersion": "1.0.0", "operationId": "world.create_plate" },
            "input": { "worldId": "world-1" },
            "prerequisiteReport": { "passed": true, "checks": [] },
            "canon": [
                {"entityId": "loc-1", "entityType": "location", "sectionId": "s-desc", "sectionKey": "description", "revision": 1, "status": "locked", "value": {"text": "A derelict station"}},
                {"entityId": "loc-1", "entityType": "location", "sectionId": "s-geo", "sectionKey": "geography", "revision": 2, "status": "locked", "value": {"text": "Rust belt"}}
            ],
            "assets": [],
            "protectedTbds": [],
            "resolvedContext": {
                "world": { "id": "world-1", "plateAssetId": "asset-1", "locationEntityId": "loc-1", "locationName": "Station" },
                "worldId": "world-1",
                "location": { "entityId": "loc-1", "name": "Station", "description": "A derelict station", "geography": "Rust belt", "visualTags": ["neon"], "rules": ["no entry"], "canonRevisionRefs": [] },
                "aesthetic": {"value": {"visual_register": "noir"}, "revisionRef": {"sectionId": "a1", "sectionKey": "aesthetic", "revision": 1}},
                "worldRules": [{"entityId": "wr-1", "name": "Gravity", "rule": "Low gravity"}],
                "productionRules": [{"id": "r1", "title": "Rule", "body": "Do not reveal"}],
                "tbdDecisions": [{"tbdId": "tbd-1", "topicSnapshot": "Secret", "noteSnapshot": "Do not reveal", "decision": "preserve_unknown"}]
            },
            "capturedAt": "2026-08-28T00:00:00Z"
        })).unwrap();

        let first = WorldPlateCompiler.compile("run-1", skill, operation, &context).unwrap();
        let second = WorldPlateCompiler.compile("run-1", skill, operation, &context).unwrap();
        assert_eq!(serde_json::to_vec(&first).unwrap(), serde_json::to_vec(&second).unwrap());
        assert_eq!(first.task, crate::workflow::execution::ExecutionTask::WorldPlate);
        assert_eq!(first.expected_output.asset_type, crate::skills::model::AssetType::WorldPlate);
        assert!(first.prompt.contains("Create a persistent environment reference plate"));
        assert!(first.prompt.contains("Treat architecture, geography, materials, recurring set dressing and permanent environmental features as canonical environment truth"));
        assert!(first.prompt.contains("Do not introduce characters unless explicitly required by a later Scene workflow"));
        assert!(first.prompt.contains("Do not over-lock: specific lens, fixed composition, exact character placement, shot-specific blocking"));
        assert!(first.prompt.contains("The image represents the place, not one particular shot inside the place"));
        assert!(first.prompt.contains("A derelict station"));
        assert!(first.prompt.contains("Rust belt"));
        assert!(first.prompt.contains("neon"));
        assert!(first.prompt.contains("Low gravity"));
        assert!(first.prompt.contains("Do not reveal"));
        assert!(first.prompt.contains("Do not attach irrelevant Character canon"));
        assert!(first.prompt.contains("PROTECTED TBD CONSTRAINTS"));
        // No provider fields
        let value = serde_json::to_value(&first).unwrap();
        assert!(value.get("provider").is_none());
        assert!(value.get("model").is_none());
        // References from canon
        assert_eq!(first.references.len(), 2);
    }

    #[test]
    fn world_plate_compiler_rejects_missing_description() {
        let registry = SkillRegistry::builtin().unwrap();
        let (skill, operation) = registry
            .find_operation("world-builder", "1.0.0", "world.create_plate")
            .unwrap();
        let context: WorkflowContextSnapshot = serde_json::from_value(serde_json::json!({
            "snapshotVersion": 1,
            "project": { "projectId": "project-1" },
            "skill": { "skillId": "world-builder", "skillVersion": "1.0.0", "operationId": "world.create_plate" },
            "input": {},
            "prerequisiteReport": { "passed": true, "checks": [] },
            "canon": [],
            "assets": [],
            "protectedTbds": [],
            "resolvedContext": {
                "world": { "id": "world-1" },
                "location": { "entityId": "loc-1", "geography": "Rust belt" }
            },
            "capturedAt": "2026-08-28T00:00:00Z"
        })).unwrap();

        let err = WorldPlateCompiler.compile("run-1", skill, operation, &context).unwrap_err();
        assert!(matches!(err, crate::error::AppError::WorkflowRunInconsistent(_)));
    }
}
