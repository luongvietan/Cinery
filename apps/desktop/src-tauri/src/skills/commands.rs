use crate::error::AppCommandError;
use crate::skills::model::SkillOperation;
use crate::skills::registry::SkillRegistry;

#[tauri::command]
pub fn list_skill_operations(
    skill_registry: tauri::State<'_, SkillRegistry>,
) -> Result<Vec<SkillOperation>, AppCommandError> {
    Ok(skill_registry
        .list()
        .into_iter()
        .flat_map(|skill| skill.operations.clone())
        .collect())
}
