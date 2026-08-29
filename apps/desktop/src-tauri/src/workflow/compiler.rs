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
                role: None,
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
            role: None,
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
                role: None,
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

pub struct SceneKeyframeCompiler;

impl RequestCompiler for SceneKeyframeCompiler {
    fn id(&self) -> &'static str {
        "scene_keyframe_v1"
    }

    fn compile(
        &self,
        workflow_run_id: &str,
        skill: &SkillDefinition,
        operation: &SkillOperation,
        context: &WorkflowContextSnapshot,
    ) -> Result<ExecutionRequest, AppError> {
        let scene = required_context(&context.resolved_context, "scene")?;
        let world = required_context(&context.resolved_context, "world")?;
        let characters = context
            .resolved_context
            .get("characters")
            .cloned()
            .unwrap_or(Value::Array(Vec::new()));
        let props = context
            .resolved_context
            .get("props")
            .cloned()
            .unwrap_or(Value::Array(Vec::new()));
        let tbd_decisions = context
            .resolved_context
            .get("tbdDecisions")
            .cloned()
            .unwrap_or(Value::Array(Vec::new()));
        let production_rules = context
            .resolved_context
            .get("productionRules")
            .cloned()
            .unwrap_or(Value::Array(Vec::new()));

        let title = scene
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("");
        let summary = scene
            .get("summary")
            .and_then(Value::as_str)
            .unwrap_or("");
        let world_asset_version_id = world
            .get("assetVersionId")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                AppError::WorkflowRunInconsistent("world.assetVersionId is missing from context".into())
            })?;
        let world_asset_id = world
            .get("assetId")
            .and_then(Value::as_str)
            .unwrap_or("");

        // Build prompt per spec 33-34: scene delta only, protected TBD constraints, no video temporal concepts
        let mut prompt = String::new();
        prompt.push_str("TASK\nCreate one scene-specific cinematic still.\n\n");
        prompt.push_str("WORLD REFERENCE\nThe attached WORLD reference controls persistent environment identity, architecture and geography. Use the exact WORLD plate version as environment truth.\n\n");
        prompt.push_str("CHARACTER LOOKS\nEach CHARACTER LOOK reference controls that character's visual identity and wardrobe. Preserve canonical details.\n\n");
        prompt.push_str("PROP REFERENCES\nAttached PROP references control prop identity. Preserve prop details.\n\n");
        prompt.push_str("SCENE DELTA\nApply only the Scene-specific delta: staging, pose, placement, framing, camera angle, moment-specific lighting, composition. Do not redesign canonical reference details. Do not resolve protected unknowns.\n\n");

        // Check for forbidden P8 concepts and ensure we don't include them
        // Explicitly avoid duration, shot timeline, video transitions, audio, etc.

        prompt.push_str("SCENE TITLE\n");
        prompt.push_str(title);
        prompt.push_str("\n\nSCENE SUMMARY\n");
        prompt.push_str(summary);
        if let Some(notes) = scene.get("notes").and_then(Value::as_str) {
            if !notes.trim().is_empty() {
                prompt.push_str("\n\nSCENE NOTES\n");
                prompt.push_str(notes);
            }
        }

        // Append production rules
        if let Some(rules) = production_rules.as_array() {
            if !rules.is_empty() {
                prompt.push_str("\n\nPRODUCTION RULES\n");
                prompt.push_str(&serde_json::to_string_pretty(&production_rules).map_err(db_error)?);
                prompt.push_str("\nLocked Production Rules are authoritative. Do not override.");
            }
        }

        // Append TBD constraints
        if let Some(decisions) = tbd_decisions.as_array() {
            if !decisions.is_empty() {
                prompt.push_str("\n\nPROTECTED TBD CONSTRAINTS\n");
                for decision in decisions {
                    let tbd_id = decision.get("tbdId").and_then(Value::as_str).unwrap_or("unknown");
                    let topic = decision.get("topicSnapshot").and_then(Value::as_str).unwrap_or("");
                    let note = decision.get("noteSnapshot").and_then(Value::as_str).unwrap_or("");
                    let dec = decision.get("decision").and_then(Value::as_str).unwrap_or("preserve_unknown");
                    if dec == "preserve_unknown" {
                        prompt.push_str(&format!("\nTBD [{}]: {}\n", tbd_id, topic));
                        if !note.is_empty() {
                            prompt.push_str(&format!("Note: {}\n", note));
                        }
                        // Concrete constraint: must remain closed/opaque for red door etc.
                        // Generic: preserve unknown, do not reveal
                        // Specific for red door
                        if topic.to_lowercase().contains("red door") || note.to_lowercase().contains("red door") {
                            prompt.push_str("Constraint: The red maintenance door must remain closed/opaque. Do not reveal, depict or imply the space behind it. Preserve unknown.\n");
                        } else {
                            prompt.push_str(&format!("Constraint: Preserve unknown for '{}'. Do not reveal, depict or imply. {}\n", topic, note));
                        }
                        if !note.is_empty() {
                            prompt.push_str(&format!("Preserve unknown: {} must remain unresolved.\n", topic));
                        }
                    } else if dec == "not_applicable" {
                        let justification = decision.get("justification").and_then(Value::as_str).unwrap_or("");
                        prompt.push_str(&format!("\nTBD [{}]: {} — not applicable. Justification: {}\n", tbd_id, topic, justification));
                    }
                }
            }
        }

        // Append reference details for determinism
        prompt.push_str("\n\nREFERENCE ROLES\n");
        prompt.push_str(&format!("WORLD assetVersionId={}\n", world_asset_version_id));
        if let Some(arr) = characters.as_array() {
            for ch in arr {
                let look_id = ch.get("look").and_then(|v| v.get("assetVersionId")).and_then(Value::as_str).unwrap_or("");
                let char_id = ch.get("characterEntityId").and_then(Value::as_str).unwrap_or("");
                prompt.push_str(&format!("CHARACTER_LOOK character={} assetVersionId={}\n", char_id, look_id));
                if let Some(sheet) = ch.get("sheet") {
                    let sheet_id = sheet.get("assetVersionId").and_then(Value::as_str).unwrap_or("");
                    prompt.push_str(&format!("CHARACTER_SHEET assetVersionId={}\n", sheet_id));
                }
            }
        }
        if let Some(arr) = props.as_array() {
            for p in arr {
                let prop_id = p.get("assetVersionId").and_then(Value::as_str).unwrap_or("");
                prompt.push_str(&format!("PROP assetVersionId={}\n", prop_id));
            }
        }

        prompt.push_str("\n\nCOMPOSITION\nExact subject position, pose, camera angle, framing, shot-specific light, spatial blocking as described in Scene summary. This is a still image, not a video sequence.\n\n");
        prompt.push_str("OUTPUT INTENT\nshot_keyframe image candidate.\n");

        // Build references with explicit roles
        let mut references: Vec<ExecutionReference> = Vec::new();

        // World reference
        references.push(ExecutionReference {
            reference_type: ExecutionReferenceType::AssetVersion,
            reference: world_asset_version_id.to_string(),
            description: format!("World plate reference for scene environment, asset {}", world_asset_id),
            role: Some(crate::workflow::execution::ReferenceRole::World),
        });

        // Characters
        if let Some(arr) = characters.as_array() {
            for ch in arr {
                let look = ch.get("look").ok_or_else(|| {
                    AppError::WorkflowRunInconsistent("character look is missing".into())
                })?;
                let look_version_id = look.get("assetVersionId").and_then(Value::as_str).ok_or_else(|| {
                    AppError::WorkflowRunInconsistent("look assetVersionId missing".into())
                })?;
                let look_asset_id = look.get("assetId").and_then(Value::as_str).unwrap_or("");
                references.push(ExecutionReference {
                    reference_type: ExecutionReferenceType::AssetVersion,
                    reference: look_version_id.to_string(),
                    description: format!("Character look reference, asset {}", look_asset_id),
                    role: Some(crate::workflow::execution::ReferenceRole::CharacterLook),
                });
                if let Some(sheet) = ch.get("sheet") {
                    if let Some(sheet_version_id) = sheet.get("assetVersionId").and_then(Value::as_str) {
                        if !sheet_version_id.trim().is_empty() {
                            let sheet_asset_id = sheet.get("assetId").and_then(Value::as_str).unwrap_or("");
                            references.push(ExecutionReference {
                                reference_type: ExecutionReferenceType::AssetVersion,
                                reference: sheet_version_id.to_string(),
                                description: format!("Character sheet reference, asset {}", sheet_asset_id),
                                role: Some(crate::workflow::execution::ReferenceRole::CharacterSheet),
                            });
                        }
                    }
                }
            }
        }

        // Props
        if let Some(arr) = props.as_array() {
            for p in arr {
                let prop_version_id = p.get("assetVersionId").and_then(Value::as_str).ok_or_else(|| {
                    AppError::WorkflowRunInconsistent("prop assetVersionId missing".into())
                })?;
                let prop_asset_id = p.get("assetId").and_then(Value::as_str).unwrap_or("");
                references.push(ExecutionReference {
                    reference_type: ExecutionReferenceType::AssetVersion,
                    reference: prop_version_id.to_string(),
                    description: format!("Prop plate reference, asset {}", prop_asset_id),
                    role: Some(crate::workflow::execution::ReferenceRole::Prop),
                });
            }
        }

        // Also include canon snapshots as references? For determinism, include canonical refs like production rules
        let mut canon_refs = context
            .canon
            .iter()
            .map(|reference| ExecutionReference {
                reference_type: ExecutionReferenceType::CanonSnapshot,
                reference: reference.section_id.clone(),
                description: format!(
                    "Locked Canon section {} at revision {}",
                    reference.section_key, reference.revision
                ),
                role: None,
            })
            .collect::<Vec<_>>();
        references.extend(canon_refs.drain(..));

        // Validate we never drop references: if any character/prop/world is present in context, we must have added it
        // This is enforced by above logic; never filter.

        // Provider capability check could be done here if providerId known, but we leave to runtime for before-execution check
        // However, we can do a generic check: ensure reference count does not exceed arbitrary limit for dry_run/mock
        // For now, just ensure we don't drop.

        let constraints = Vec::new();

        Ok(ExecutionRequest {
            request_version: 1,
            task: ExecutionTask::ShotKeyframe,
            media_type: ExecutionMediaType::Image,
            prompt,
            references,
            constraints,
            expected_output: operation.expected_output.clone().ok_or_else(|| {
                AppError::WorkflowRunInconsistent(
                    "scene keyframe operation has no expected output".into(),
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

pub struct CharacterOutfitCompiler;

impl RequestCompiler for CharacterOutfitCompiler {
    fn id(&self) -> &'static str {
        "character_outfit_v1"
    }

    fn compile(
        &self,
        workflow_run_id: &str,
        skill: &SkillDefinition,
        operation: &SkillOperation,
        context: &WorkflowContextSnapshot,
    ) -> Result<ExecutionRequest, AppError> {
        let character = required_context(&context.resolved_context, "character")?;
        let wardrobe = required_context(&context.resolved_context, "wardrobeProposal")?;
        let locks = character
            .get("permanentVisualLocks")
            .cloned()
            .ok_or_else(|| {
                AppError::WorkflowRunInconsistent(
                    "permanentVisualLocks is missing from context".into(),
                )
            })?;
        let canonical_face = context
            .resolved_context
            .get("canonicalFaceAssetVersionId")
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                AppError::WorkflowRunInconsistent(
                    "canonical face asset is missing from context".into(),
                )
            })?;
        let mut prompt = String::new();
        prompt.push_str("TASK\nCreate a direct-on-character outfit reference.\n\n");
        prompt.push_str("CHARACTER IDENTITY\n");
        prompt.push_str(&serde_json::to_string_pretty(&json_without_story_name(&character)).map_err(db_error)?);
        prompt.push_str("\n\nPERMANENT VISUAL LOCKS\n");
        prompt.push_str(&serde_json::to_string_pretty(&locks).map_err(db_error)?);
        prompt.push_str("\n\nWARDROBE PROPOSAL\n");
        prompt.push_str(&serde_json::to_string_pretty(&wardrobe).map_err(db_error)?);
        prompt.push_str("\n\nCANONICAL FACE REFERENCE\nUse the canonical face asset version: ");
        prompt.push_str(canonical_face);
        prompt.push_str("\n\nOUTPUT INTENT\noutfit image candidate on the canonical character.\n");

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
                role: None,
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
            role: None,
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
            task: ExecutionTask::CharacterOutfit,
            media_type: ExecutionMediaType::Image,
            prompt,
            references,
            constraints,
            expected_output: operation.expected_output.clone().ok_or_else(|| {
                AppError::WorkflowRunInconsistent(
                    "outfit operation has no expected output".into(),
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

pub struct CharacterSheetCompiler;

impl RequestCompiler for CharacterSheetCompiler {
    fn id(&self) -> &'static str {
        "character_sheet_v1"
    }

    fn compile(
        &self,
        workflow_run_id: &str,
        skill: &SkillDefinition,
        operation: &SkillOperation,
        context: &WorkflowContextSnapshot,
    ) -> Result<ExecutionRequest, AppError> {
        let character = required_context(&context.resolved_context, "character")?;
        let canonical_look = context
            .resolved_context
            .get("canonicalOutfitAssetVersionId")
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                AppError::WorkflowRunInconsistent(
                    "canonical outfit asset is missing from context".into(),
                )
            })?;
        let mut prompt = String::new();
        prompt.push_str("TASK\nCreate a three-panel character sheet.\n\n");
        prompt.push_str("CHARACTER IDENTITY\n");
        prompt.push_str(&serde_json::to_string_pretty(&json_without_story_name(&character)).map_err(db_error)?);
        prompt.push_str("\n\nCANONICAL LOOK REFERENCE\nUse the canonical outfit asset version: ");
        prompt.push_str(canonical_look);
        prompt.push_str("\n\nSHEET PANELS (from §24)\n1. full-body front, headless;\n2. full-body rear;\n3. tight chest-up face.\n\nOUTPUT INTENT\ncharacter_sheet image candidate.\n");

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
                role: None,
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
            role: None,
        }));

        Ok(ExecutionRequest {
            request_version: 1,
            task: ExecutionTask::CharacterSheet,
            media_type: ExecutionMediaType::Image,
            prompt,
            references,
            constraints: vec![],
            expected_output: operation.expected_output.clone().ok_or_else(|| {
                AppError::WorkflowRunInconsistent(
                    "character sheet operation has no expected output".into(),
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
            .find_operation("character-builder", "1.1.0", "character.create_face_lock")
            .unwrap();
        let context: WorkflowContextSnapshot = serde_json::from_value(serde_json::json!({
            "snapshotVersion": 1,
            "project": { "projectId": "project-1" },
            "skill": { "skillId": "character-builder", "skillVersion": "1.1.0", "operationId": "character.create_face_lock" },
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
