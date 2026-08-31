use crate::canon::model::{CanonEntityRecord, CanonEntityType, CanonSectionRecord, CanonTbdRecord};
use crate::canon::{repository, schema};
use crate::db;
use crate::error::AppError;
use crate::project::service::ProjectService;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryBibleExportResult {
    pub relative_path: String,
    pub byte_size: usize,
}

pub fn export_story_bible(project_root: &Path) -> Result<StoryBibleExportResult, AppError> {
    let project = ProjectService::open(project_root)?;
    let conn = db::open_existing_connection(&project_root.join("project.db"))?;
    let entities = repository::list_entities(&conn, &project.id, None)?;
    let mut sections = HashMap::new();
    for entity in &entities {
        sections.insert(
            entity.id.clone(),
            repository::list_sections(&conn, &entity.id)?,
        );
    }
    let tbds = repository::list_tbds(&conn, &project.id)?;
    let markdown = render(&project.name, &entities, &sections, &tbds);
    let directory = project_root.join("canon");
    fs::create_dir_all(&directory).map_err(|e| AppError::CanonExport(e.to_string()))?;
    let target = directory.join("story-bible.md");
    let temp = directory.join("story-bible.md.tmp");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temp)
        .map_err(|e| AppError::CanonExport(e.to_string()))?;
    file.write_all(markdown.as_bytes())
        .map_err(|e| AppError::CanonExport(e.to_string()))?;
    file.sync_all()
        .map_err(|e| AppError::CanonExport(e.to_string()))?;
    drop(file);
    if target.exists() {
        fs::remove_file(&target).map_err(|e| AppError::CanonExport(e.to_string()))?;
    }
    fs::rename(&temp, &target).map_err(|e| AppError::CanonExport(e.to_string()))?;
    Ok(StoryBibleExportResult {
        relative_path: "canon/story-bible.md".into(),
        byte_size: markdown.len(),
    })
}

fn render(
    project_name: &str,
    entities: &[CanonEntityRecord],
    sections: &HashMap<String, Vec<CanonSectionRecord>>,
    tbds: &[CanonTbdRecord],
) -> String {
    let story_sections = entities
        .iter()
        .find(|entity| entity.entity_type == "story")
        .and_then(|entity| sections.get(&entity.id))
        .map(|items| by_key(items))
        .unwrap_or_default();
    let mut output = format!("# {project_name} — Story Bible\n\n");
    render_story_section(&mut output, "## 1. Premise", story_sections.get("premise"));
    render_story_section(&mut output, "## 2. Thesis", story_sections.get("thesis"));
    render_story_section(
        &mut output,
        "## 3. World / Timeline",
        story_sections.get("timeline"),
    );
    render_story_section(
        &mut output,
        "## 4. Aesthetic",
        story_sections.get("aesthetic"),
    );
    output.push_str("## 5. Factions\n\n");
    render_entities(&mut output, entities, sections, "faction");
    output.push_str("## 6. Locations\n\n");
    render_entities(&mut output, entities, sections, "location");
    output.push_str("## 7. World Rules\n\n");
    render_entities(&mut output, entities, sections, "world_rule");
    output.push_str("## 8. Characters\n\n");
    render_characters(&mut output, entities, sections);
    render_story_section(
        &mut output,
        "## 9. Relationships and Ensemble Dynamics",
        story_sections.get("relationships"),
    );
    render_story_section(
        &mut output,
        "## 10. Structural Engines",
        story_sections.get("structural_engines"),
    );
    output.push_str("## 11. Production Rules\n\n");
    let production = entities
        .iter()
        .find(|entity| entity.entity_type == "production_rules")
        .and_then(|entity| sections.get(&entity.id))
        .and_then(|items| items.iter().find(|item| item.key == "rules"));
    render_section(&mut output, production);
    render_story_section(
        &mut output,
        "## 12. When This Canon Is Active",
        story_sections.get("active_skill_rules"),
    );
    output.push_str("## Open TBDs\n\n");
    for tbd in tbds.iter().filter(|tbd| tbd.status == "open") {
        output.push_str(&format!(
            "- **{}{}**\n",
            if tbd.protected { "[PROTECTED] " } else { "" },
            tbd.topic
        ));
        if let Some(entity_id) = &tbd.canon_entity_id {
            if let Some(entity) = entities.iter().find(|entity| &entity.id == entity_id) {
                output.push_str(&format!(
                    "  Scope: {} — {}{}\n",
                    display_type(&entity.entity_type),
                    entity.name,
                    tbd.section_key
                        .as_ref()
                        .map(|key| format!(" / {key}"))
                        .unwrap_or_default()
                ));
            }
        }
        if let Some(note) = &tbd.note {
            output.push_str(&format!("  Note: {note}\n"));
        }
        output.push('\n');
    }
    output
}

fn render_story_section(output: &mut String, heading: &str, section: Option<&CanonSectionRecord>) {
    output.push_str(&format!("{heading}\n\n"));
    render_section(output, section);
}
fn render_section(output: &mut String, section: Option<&CanonSectionRecord>) {
    match section {
        Some(section) => output.push_str(&format!(
            "**Status:** {}\n\n{}\n\n",
            section.status.to_uppercase(),
            value_text(&section.value)
        )),
        None => output.push_str("[TBD]\n\n"),
    }
}
fn value_text(value: &serde_json::Value) -> String {
    if let Some(text) = value.get("text").and_then(|item| item.as_str()) {
        return if text.is_empty() {
            "[TBD]".into()
        } else {
            text.into()
        };
    }
    if let Some(entries) = value.get("entries").and_then(|item| item.as_array()) {
        return entries
            .iter()
            .map(|item| {
                format!(
                    "- **{}** — {}",
                    string_value(item, "label"),
                    string_value(item, "description")
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    if let Some(items) = value.get("rules").and_then(|item| item.as_array()) {
        return items
            .iter()
            .map(|item| {
                if item.is_object() {
                    format!(
                        "- **{}** — {}",
                        string_value(item, "title"),
                        string_value(item, "body")
                    )
                } else {
                    format!("- {}", item.as_str().unwrap_or("[TBD]"))
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    if let Some(locks) = value.get("locks").and_then(|item| item.as_array()) {
        return locks
            .iter()
            .map(|item| {
                format!(
                    "- [{}] {} — {}",
                    string_value(item, "severity").to_uppercase(),
                    string_value(item, "key"),
                    string_value(item, "description")
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "[TBD]".into())
}
fn string_value(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|item| item.as_str())
        .filter(|item| !item.is_empty())
        .unwrap_or("[TBD]")
        .to_string()
}
fn by_key(items: &[CanonSectionRecord]) -> HashMap<String, CanonSectionRecord> {
    items
        .iter()
        .map(|item| (item.key.clone(), item.clone()))
        .collect()
}
fn render_entities(
    output: &mut String,
    entities: &[CanonEntityRecord],
    sections: &HashMap<String, Vec<CanonSectionRecord>>,
    entity_type: &str,
) {
    let mut matching: Vec<_> = entities
        .iter()
        .filter(|entity| entity.entity_type == entity_type)
        .collect();
    matching.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    for entity in &matching {
        output.push_str(&format!("### {}\n\n", entity.name));
        if let Some(items) = sections.get(&entity.id) {
            if let Some(kind) = CanonEntityType::parse(entity_type) {
                for key in schema::section_keys(kind) {
                    if let Some(section) = items.iter().find(|item| item.key == *key) {
                        output.push_str(&format!(
                            "#### {} — {}\n\n{}\n\n",
                            title_for(key),
                            section.status.to_uppercase(),
                            value_text(&section.value)
                        ));
                    } else {
                        output.push_str(&format!("#### {} — DRAFT\n\n[TBD]\n\n", title_for(key)));
                    }
                }
            }
        } else if let Some(kind) = CanonEntityType::parse(entity_type) {
            for key in schema::section_keys(kind) {
                output.push_str(&format!("#### {} — DRAFT\n\n[TBD]\n\n", title_for(key)));
            }
        }
    }
    if matching.is_empty() {
        output.push_str("[TBD]\n\n");
    }
}
fn render_characters(
    output: &mut String,
    entities: &[CanonEntityRecord],
    sections: &HashMap<String, Vec<CanonSectionRecord>>,
) {
    let mut characters: Vec<_> = entities
        .iter()
        .filter(|entity| entity.entity_type == "character")
        .collect();
    characters.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    for entity in &characters {
        let map = sections
            .get(&entity.id)
            .map(|items| by_key(items))
            .unwrap_or_default();
        let role = map
            .get("role_tag")
            .map(|section| value_text(&section.value))
            .unwrap_or_else(|| "[TBD]".into());
        output.push_str(&format!(
            "### {} — *{}*\n\n",
            entity.name.to_uppercase(),
            role
        ));
        for (key, label) in [
            ("visual_summary", "Visual"),
            ("function", "Function in the story"),
            ("backstory", "Backstory"),
            ("psychology", "Present-tense psychology"),
            ("speech", "Speech"),
            ("movement", "Movement"),
            ("stillness", "Stillness"),
        ] {
            let text = map
                .get(key)
                .map(|section| value_text(&section.value))
                .unwrap_or_else(|| "[TBD]".into());
            let text = if ["speech", "movement", "stillness"].contains(&key) && text != "[TBD]" {
                format!("\"{text}\"")
            } else {
                text
            };
            output.push_str(&format!("**{label}:** {text}\n\n"));
        }
        output.push_str("**Permanent visual locks:**\n");
        output.push_str(&format!(
            "{}\n\n",
            map.get("visual_locks")
                .map(|section| value_text(&section.value))
                .unwrap_or_else(|| "- [TBD]".into())
        ));
        output.push_str(&format!(
            "**Sub-beats:**\n{}\n\n",
            map.get("sub_beats")
                .map(|section| value_text(&section.value))
                .unwrap_or_else(|| "[TBD]".into())
        ));
    }
    if characters.is_empty() {
        output.push_str("[TBD]\n\n");
    }
}
fn title_for(key: &str) -> String {
    key.split('_')
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}
fn display_type(value: &str) -> &str {
    match value {
        "world_rule" => "World Rule",
        "location" => "Location",
        "character" => "Character",
        "faction" => "Faction",
        _ => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::service::CanonService;
    use crate::canon::tbd;
    use crate::project::service::ProjectService;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn export_is_deterministic_and_contains_locked_state_and_open_tbd() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("red-door");
        ProjectService::create(&root, "Red Door").unwrap();
        let singletons = CanonService::ensure_singletons(&root).unwrap();
        let premise = CanonService::upsert_section(
            &root,
            &singletons.story.id,
            "premise",
            serde_json::json!({"text":"A lone operator receives her future voice."}),
            None,
        )
        .unwrap();
        CanonService::lock_section(&root, &premise.id, None).unwrap();
        CanonService::upsert_section(
            &root,
            &singletons.story.id,
            "thesis",
            serde_json::json!({"text":"Unknown canon stays unknown."}),
            None,
        )
        .unwrap();
        let character = CanonService::create_entity(
            &root,
            crate::canon::model::CanonEntityType::Character,
            "Mara Keene",
        )
        .unwrap();
        let role = CanonService::upsert_section(
            &root,
            &character.id,
            "role_tag",
            serde_json::json!({"text":"The Verifier"}),
            None,
        )
        .unwrap();
        CanonService::lock_section(&root, &role.id, None).unwrap();
        let locks = CanonService::upsert_section(&root, &character.id, "visual_locks", serde_json::json!({"locks":[{"id":"scar","key":"right_eyebrow_scar","description":"A healed scar.","severity":"required","validatorHint":null}]}), None).unwrap();
        CanonService::lock_section(&root, &locks.id, None).unwrap();
        tbd::create(
            &root,
            None,
            None,
            "What is behind the red door?",
            None,
            true,
        )
        .unwrap();
        export_story_bible(&root).unwrap();
        let first = fs::read(root.join("canon/story-bible.md")).unwrap();
        export_story_bible(&root).unwrap();
        let second = fs::read(root.join("canon/story-bible.md")).unwrap();
        assert_eq!(first, second);
        let text = String::from_utf8(first).unwrap();
        assert!(text.contains("**Status:** LOCKED"));
        assert!(text.contains("[PROTECTED] What is behind the red door?"));
        assert!(text.contains("right_eyebrow_scar"));
        assert!(text.contains("**Status:** DRAFT"));
    }
}
