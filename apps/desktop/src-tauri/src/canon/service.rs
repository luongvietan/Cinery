use crate::canon::model::{
    CanonEntityRecord, CanonEntityType, CanonSectionRecord, CanonSectionRevisionRecord,
    CanonSingletonsDto,
};
use crate::canon::{repository, schema};
use crate::db;
use crate::error::AppError;
use crate::project::service::ProjectService;
use chrono::Utc;
use rusqlite::OptionalExtension;
use std::path::Path;
use ulid::Ulid;

pub struct CanonService;

impl CanonService {
    pub fn ensure_singletons(project_root: &Path) -> Result<CanonSingletonsDto, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        let tx = conn.transaction().map_err(db_error)?;
        let story =
            find_or_insert_singleton(&tx, &project.id, CanonEntityType::Story, "Story", "story")?;
        let production_rules = find_or_insert_singleton(
            &tx,
            &project.id,
            CanonEntityType::ProductionRules,
            "Production Rules",
            "production-rules",
        )?;
        tx.commit().map_err(db_error)?;
        Ok(CanonSingletonsDto {
            story,
            production_rules,
        })
    }

    pub fn create_entity(
        project_root: &Path,
        entity_type: CanonEntityType,
        name: &str,
    ) -> Result<CanonEntityRecord, AppError> {
        if matches!(
            entity_type,
            CanonEntityType::Story | CanonEntityType::ProductionRules
        ) {
            return Err(AppError::CanonSingletonTypeRequired);
        }
        let project = ProjectService::open(project_root)?;
        let trimmed_name = name.trim();
        if trimmed_name.is_empty() || trimmed_name.chars().count() > 160 {
            return Err(AppError::InvalidCanonEntityName);
        }
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        let tx = conn.transaction().map_err(db_error)?;
        let slug = allocate_slug(&tx, &project.id, entity_type, trimmed_name)?;
        let now = Utc::now().to_rfc3339();
        let record = CanonEntityRecord {
            id: Ulid::new().to_string(),
            project_id: project.id,
            entity_type: entity_type.as_str().to_string(),
            name: trimmed_name.to_string(),
            slug,
            created_at: now.clone(),
            updated_at: now,
        };
        repository::insert_entity(&tx, &record)?;
        tx.commit().map_err(db_error)?;
        Ok(record)
    }

    pub fn list_entities(
        project_root: &Path,
        entity_type: Option<CanonEntityType>,
    ) -> Result<Vec<CanonEntityRecord>, AppError> {
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        repository::list_entities(&conn, &project.id, entity_type)
    }

    pub fn get_entity(
        project_root: &Path,
        entity_id: &str,
    ) -> Result<CanonEntityDetailDto, AppError> {
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        let entity = repository::get_entity(&conn, entity_id)?;
        if entity.project_id != project.id {
            return Err(AppError::CanonEntityNotFound);
        }
        let mut sections = repository::list_sections(&conn, entity_id)?;
        if let Some(entity_type) = CanonEntityType::from_str(&entity.entity_type) {
            let order = schema::section_keys(entity_type);
            sections.sort_by_key(|section| {
                order
                    .iter()
                    .position(|key| *key == section.key)
                    .unwrap_or(usize::MAX)
            });
        }
        Ok(CanonEntityDetailDto { entity, sections })
    }

    pub fn upsert_section(
        project_root: &Path,
        entity_id: &str,
        section_key: &str,
        value: serde_json::Value,
        reason: Option<String>,
    ) -> Result<CanonSectionRecord, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        let tx = conn.transaction().map_err(db_error)?;
        let entity = repository::get_entity(&tx, entity_id)?;
        if entity.project_id != project.id {
            return Err(AppError::CanonEntityNotFound);
        }
        let entity_type =
            CanonEntityType::from_str(&entity.entity_type).ok_or(AppError::CanonEntityNotFound)?;
        schema::validate_section_value(entity_type, section_key, &value)?;
        let now = Utc::now().to_rfc3339();
        let existing = repository::get_section_by_key(&tx, entity_id, section_key)?;
        let section = if let Some(mut section) = existing {
            if section.status == "locked" {
                return Err(AppError::CanonSectionLocked);
            }
            section.revision += 1;
            section.value = value;
            section.updated_at = now.clone();
            repository::update_section(&tx, &section)?;
            insert_revision(&tx, &section, "edit", reason, now)?;
            section
        } else {
            let section = CanonSectionRecord {
                id: Ulid::new().to_string(),
                entity_id: entity_id.to_string(),
                key: section_key.to_string(),
                value,
                status: "draft".to_string(),
                revision: 1,
                created_at: now.clone(),
                updated_at: now.clone(),
                locked_at: None,
            };
            repository::insert_section(&tx, &section)?;
            insert_revision(&tx, &section, "create", reason, now)?;
            section
        };
        tx.commit().map_err(db_error)?;
        Ok(section)
    }

    pub fn lock_section(
        project_root: &Path,
        section_id: &str,
        reason: Option<String>,
    ) -> Result<CanonSectionRecord, AppError> {
        Self::set_section_lock(project_root, section_id, true, reason)
    }

    pub fn unlock_section(
        project_root: &Path,
        section_id: &str,
        reason: Option<String>,
    ) -> Result<CanonSectionRecord, AppError> {
        Self::set_section_lock(project_root, section_id, false, reason)
    }

    fn set_section_lock(
        project_root: &Path,
        section_id: &str,
        locked: bool,
        reason: Option<String>,
    ) -> Result<CanonSectionRecord, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        let tx = conn.transaction().map_err(db_error)?;
        let section = repository::get_section(&tx, section_id)?;
        let entity = repository::get_entity(&tx, &section.entity_id)?;
        if entity.project_id != project.id {
            return Err(AppError::CanonSectionNotFound);
        }
        if locked && section.status == "locked" {
            return Err(AppError::CanonSectionAlreadyLocked);
        }
        if !locked && section.status != "locked" {
            return Err(AppError::CanonSectionAlreadyUnlocked);
        }
        let now = Utc::now().to_rfc3339();
        let mut updated = section;
        updated.status = if locked { "locked" } else { "draft" }.to_string();
        updated.revision += 1;
        updated.updated_at = now.clone();
        updated.locked_at = if locked { Some(now.clone()) } else { None };
        repository::update_section(&tx, &updated)?;
        insert_revision(
            &tx,
            &updated,
            if locked { "lock" } else { "unlock" },
            reason,
            now,
        )?;
        tx.commit().map_err(db_error)?;
        Ok(updated)
    }

    pub fn list_section_revisions(
        project_root: &Path,
        section_id: &str,
    ) -> Result<Vec<CanonSectionRevisionRecord>, AppError> {
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        let section = repository::get_section(&conn, section_id)?;
        let entity = repository::get_entity(&conn, &section.entity_id)?;
        if entity.project_id != project.id {
            return Err(AppError::CanonSectionNotFound);
        }
        repository::list_revisions(&conn, section_id)
    }

    pub fn get_locked_character_visual_locks(
        project_root: &Path,
        character_entity_id: &str,
    ) -> Result<Vec<VisualLockDto>, AppError> {
        let detail = Self::get_entity(project_root, character_entity_id)?;
        if detail.entity.entity_type != "character" {
            return Err(AppError::CanonEntityNotFound);
        }
        let Some(section) = detail
            .sections
            .iter()
            .find(|section| section.key == "visual_locks" && section.status == "locked")
        else {
            return Ok(Vec::new());
        };
        let locks = section
            .value
            .get("locks")
            .and_then(|value| value.as_array())
            .ok_or(AppError::InvalidCanonSectionValue)?;
        locks
            .iter()
            .map(|lock| {
                serde_json::from_value(lock.clone()).map_err(|_| AppError::InvalidCanonSectionValue)
            })
            .collect()
    }

    pub fn list_locked_world_rules(
        project_root: &Path,
    ) -> Result<Vec<LockedWorldRuleDto>, AppError> {
        let entities = Self::list_entities(project_root, Some(CanonEntityType::WorldRule))?;
        let mut result = Vec::new();
        for entity in entities {
            let detail = Self::get_entity(project_root, &entity.id)?;
            let Some(rule) = detail
                .sections
                .iter()
                .find(|section| section.key == "rule" && section.status == "locked")
            else {
                continue;
            };
            result.push(LockedWorldRuleDto {
                entity_id: entity.id,
                name: entity.name,
                rule: rule
                    .value
                    .get("text")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_string(),
            });
        }
        Ok(result)
    }

    pub fn get_locked_production_rules(
        project_root: &Path,
    ) -> Result<Vec<ProductionRuleDto>, AppError> {
        let singletons = Self::ensure_singletons(project_root)?;
        let detail = Self::get_entity(project_root, &singletons.production_rules.id)?;
        let Some(section) = detail
            .sections
            .iter()
            .find(|section| section.key == "rules" && section.status == "locked")
        else {
            return Ok(Vec::new());
        };
        let rules = section
            .value
            .get("rules")
            .and_then(|value| value.as_array())
            .ok_or(AppError::InvalidCanonSectionValue)?;
        rules
            .iter()
            .map(|rule| {
                serde_json::from_value(rule.clone()).map_err(|_| AppError::InvalidCanonSectionValue)
            })
            .collect()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonEntityDetailDto {
    pub entity: CanonEntityRecord,
    pub sections: Vec<CanonSectionRecord>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualLockDto {
    pub id: String,
    pub key: String,
    pub description: String,
    pub severity: String,
    pub validator_hint: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockedWorldRuleDto {
    pub entity_id: String,
    pub name: String,
    pub rule: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionRuleDto {
    pub id: String,
    pub title: String,
    pub body: String,
}

fn find_or_insert_singleton(
    tx: &rusqlite::Transaction<'_>,
    project_id: &str,
    entity_type: CanonEntityType,
    name: &str,
    slug: &str,
) -> Result<CanonEntityRecord, AppError> {
    let existing = tx
        .query_row(
            "SELECT id, project_id, type, name, slug, created_at, updated_at
             FROM canon_entities WHERE project_id = ?1 AND type = ?2 LIMIT 1",
            rusqlite::params![project_id, entity_type.as_str()],
            |row| {
                Ok(CanonEntityRecord {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    entity_type: row.get(2)?,
                    name: row.get(3)?,
                    slug: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(db_error)?;
    if let Some(record) = existing {
        return Ok(record);
    }
    let now = Utc::now().to_rfc3339();
    let record = CanonEntityRecord {
        id: Ulid::new().to_string(),
        project_id: project_id.to_string(),
        entity_type: entity_type.as_str().to_string(),
        name: name.to_string(),
        slug: slug.to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    repository::insert_entity(tx, &record)?;
    Ok(record)
}

fn allocate_slug(
    tx: &rusqlite::Transaction<'_>,
    project_id: &str,
    entity_type: CanonEntityType,
    name: &str,
) -> Result<String, AppError> {
    let base = slugify(name);
    let base = if base.is_empty() {
        "entity".to_string()
    } else {
        base
    };
    let mut candidate = base.clone();
    let mut suffix = 2;
    while tx
        .query_row(
            "SELECT COUNT(*) FROM canon_entities WHERE project_id = ?1 AND type = ?2 AND slug = ?3",
            rusqlite::params![project_id, entity_type.as_str(), candidate],
            |row| row.get::<_, i64>(0),
        )
        .map_err(db_error)?
        > 0
    {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    Ok(candidate)
}

fn slugify(value: &str) -> String {
    let mut result = String::new();
    let mut pending_dash = false;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            if pending_dash && !result.is_empty() {
                result.push('-');
            }
            pending_dash = false;
            result.push(ch);
        } else if !result.is_empty() {
            pending_dash = true;
        }
    }
    result
}

fn insert_revision(
    tx: &rusqlite::Transaction<'_>,
    section: &CanonSectionRecord,
    change_kind: &str,
    reason: Option<String>,
    created_at: String,
) -> Result<(), AppError> {
    repository::insert_revision(
        tx,
        &CanonSectionRevisionRecord {
            id: Ulid::new().to_string(),
            section_id: section.id.clone(),
            revision: section.revision,
            value: section.value.clone(),
            status: section.status.clone(),
            change_kind: change_kind.to_string(),
            reason,
            created_at,
        },
    )
}

fn db_error(error: rusqlite::Error) -> AppError {
    AppError::Database(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::service::ProjectService;
    use tempfile::tempdir;

    fn project() -> (tempfile::TempDir, std::path::PathBuf) {
        let temp = tempdir().unwrap();
        let root = temp.path().join("red-door");
        ProjectService::create(&root, "Red Door").unwrap();
        (temp, root)
    }

    #[test]
    fn ensure_singletons_is_idempotent() {
        let (_temp, root) = project();
        let first = CanonService::ensure_singletons(&root).unwrap();
        let second = CanonService::ensure_singletons(&root).unwrap();
        assert_eq!(first.story.id, second.story.id);
        assert_eq!(first.production_rules.id, second.production_rules.id);
        assert_eq!(CanonService::list_entities(&root, None).unwrap().len(), 2);
    }

    #[test]
    fn creates_collision_safe_slugs() {
        let (_temp, root) = project();
        let first =
            CanonService::create_entity(&root, CanonEntityType::Character, "Mara Keene").unwrap();
        let second =
            CanonService::create_entity(&root, CanonEntityType::Character, "Mara Keene").unwrap();
        assert_eq!(first.slug, "mara-keene");
        assert_eq!(second.slug, "mara-keene-2");
    }

    #[test]
    fn section_transitions_are_revisioned_and_locked_edits_fail() {
        let (_temp, root) = project();
        let story = CanonService::ensure_singletons(&root).unwrap().story;
        let mut section = CanonService::upsert_section(
            &root,
            &story.id,
            "premise",
            serde_json::json!({"text": "First"}),
            Some("initial".into()),
        )
        .unwrap();
        assert_eq!((section.revision, section.status.as_str()), (1, "draft"));
        section = CanonService::upsert_section(
            &root,
            &story.id,
            "premise",
            serde_json::json!({"text": "Second"}),
            None,
        )
        .unwrap();
        section = CanonService::lock_section(&root, &section.id, None).unwrap();
        assert_eq!((section.revision, section.status.as_str()), (3, "locked"));
        assert!(matches!(
            CanonService::upsert_section(
                &root,
                &story.id,
                "premise",
                serde_json::json!({"text": "Third"}),
                None
            ),
            Err(AppError::CanonSectionLocked)
        ));
        CanonService::unlock_section(&root, &section.id, None).unwrap();
        section = CanonService::upsert_section(
            &root,
            &story.id,
            "premise",
            serde_json::json!({"text": "Fourth"}),
            None,
        )
        .unwrap();
        assert_eq!(section.revision, 5);
        let history = CanonService::list_section_revisions(&root, &section.id).unwrap();
        assert_eq!(
            history
                .iter()
                .map(|revision| revision.revision)
                .collect::<Vec<_>>(),
            vec![5, 4, 3, 2, 1]
        );
        assert_eq!(history[0].change_kind, "edit");
        assert_eq!(history[2].change_kind, "lock");
    }
}
