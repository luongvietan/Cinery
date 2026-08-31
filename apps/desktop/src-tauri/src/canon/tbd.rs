use crate::canon::model::{CanonEntityType, CanonTbdRecord};
use crate::canon::{repository, schema};
use crate::db;
use crate::error::AppError;
use crate::project::service::ProjectService;
use chrono::Utc;
use std::path::Path;
use ulid::Ulid;

pub fn create(
    project_root: &Path,
    entity_id: Option<&str>,
    section_key: Option<&str>,
    topic: &str,
    note: Option<String>,
    protected: bool,
) -> Result<CanonTbdRecord, AppError> {
    let project = ProjectService::open(project_root)?;
    let conn = db::open_existing_connection(&project_root.join("project.db"))?;
    let entity = match entity_id {
        Some(id) => {
            let entity = repository::get_entity(&conn, id)?;
            if entity.project_id != project.id {
                return Err(AppError::CanonTbdEntityProjectMismatch);
            }
            Some(entity)
        }
        None => {
            if section_key.is_some() {
                return Err(AppError::CanonTbdSectionMismatch);
            }
            None
        }
    };
    if let Some(key) = section_key {
        let entity = entity.as_ref().ok_or(AppError::CanonTbdSectionMismatch)?;
        let entity_type =
            CanonEntityType::parse(&entity.entity_type).ok_or(AppError::CanonTbdSectionMismatch)?;
        if !schema::section_keys(entity_type).contains(&key) {
            return Err(AppError::CanonTbdSectionMismatch);
        }
        let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM canon_sections WHERE canon_entity_id = ?1 AND section_key = ?2)", rusqlite::params![entity.id, key], |row| row.get(0)).map_err(|e| AppError::Database(e.to_string()))?;
        if !exists {
            return Err(AppError::CanonTbdSectionMismatch);
        }
    }
    let topic = topic.trim();
    if topic.is_empty() || topic.chars().count() > 240 {
        return Err(AppError::InvalidCanonTbdTopic);
    }
    let now = Utc::now().to_rfc3339();
    let record = CanonTbdRecord {
        id: Ulid::new().to_string(),
        project_id: project.id,
        canon_entity_id: entity.map(|item| item.id),
        section_key: section_key.map(str::to_string),
        topic: topic.to_string(),
        note: note
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty()),
        protected,
        status: "open".into(),
        resolution_text: None,
        created_at: now.clone(),
        updated_at: now,
        resolved_at: None,
    };
    repository::insert_tbd(&conn, &record)?;
    Ok(record)
}

pub fn list(project_root: &Path) -> Result<Vec<CanonTbdRecord>, AppError> {
    let project = ProjectService::open(project_root)?;
    let conn = db::open_existing_connection(&project_root.join("project.db"))?;
    repository::list_tbds(&conn, &project.id)
}

pub fn resolve(
    project_root: &Path,
    tbd_id: &str,
    resolution_text: &str,
) -> Result<CanonTbdRecord, AppError> {
    mutate(project_root, tbd_id, |record| {
        let text = resolution_text.trim();
        if text.is_empty() {
            return Err(AppError::InvalidCanonTbdResolution);
        }
        record.status = "resolved".into();
        record.resolution_text = Some(text.into());
        record.resolved_at = Some(Utc::now().to_rfc3339());
        Ok(())
    })
}
pub fn reopen(project_root: &Path, tbd_id: &str) -> Result<CanonTbdRecord, AppError> {
    mutate(project_root, tbd_id, |record| {
        record.status = "open".into();
        record.resolution_text = None;
        record.resolved_at = None;
        Ok(())
    })
}
pub fn list_open_protected(project_root: &Path) -> Result<Vec<CanonTbdRecord>, AppError> {
    Ok(list(project_root)?
        .into_iter()
        .filter(|record| record.status == "open" && record.protected)
        .collect())
}

fn mutate<F>(project_root: &Path, tbd_id: &str, action: F) -> Result<CanonTbdRecord, AppError>
where
    F: FnOnce(&mut CanonTbdRecord) -> Result<(), AppError>,
{
    let project = ProjectService::open(project_root)?;
    let conn = db::open_existing_connection(&project_root.join("project.db"))?;
    let mut record = repository::get_tbd(&conn, tbd_id)?;
    if record.project_id != project.id {
        return Err(AppError::CanonTbdNotFound);
    }
    action(&mut record)?;
    record.updated_at = Utc::now().to_rfc3339();
    repository::update_tbd(&conn, &record)?;
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn project(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let temp = tempdir().unwrap();
        let root = temp.path().join(name);
        ProjectService::create(&root, name).unwrap();
        (temp, root)
    }

    #[test]
    fn creates_and_reopens_protected_tbd() {
        let (_temp, root) = project("Red Door");
        let tbd = create(
            &root,
            None,
            None,
            "What is behind the red door?",
            Some("Do not visualize before reveal.".into()),
            true,
        )
        .unwrap();
        assert_eq!(tbd.status, "open");
        assert!(tbd.protected);
        let resolved = resolve(&root, &tbd.id, "The room is intentionally withheld.").unwrap();
        assert_eq!(resolved.status, "resolved");
        let reopened = reopen(&root, &tbd.id).unwrap();
        assert_eq!(reopened.status, "open");
        assert!(reopened.resolution_text.is_none());
        assert!(reopened.protected);
    }

    #[test]
    fn section_scoped_tbd_requires_existing_section() {
        let (_temp, root) = project("Red Door");
        let story = crate::canon::service::CanonService::ensure_singletons(&root)
            .unwrap()
            .story;
        assert!(matches!(
            create(
                &root,
                Some(&story.id),
                Some("premise"),
                "Unknown",
                None,
                true
            ),
            Err(AppError::CanonTbdSectionMismatch)
        ));
        crate::canon::service::CanonService::upsert_section(
            &root,
            &story.id,
            "premise",
            serde_json::json!({"text":"A premise"}),
            None,
        )
        .unwrap();
        let tbd = create(
            &root,
            Some(&story.id),
            Some("premise"),
            "Unknown detail",
            None,
            true,
        )
        .unwrap();
        assert_eq!(tbd.section_key.as_deref(), Some("premise"));
    }

    #[test]
    fn protected_query_excludes_unprotected_and_resolved() {
        let (_temp, root) = project("Red Door");
        let first = create(&root, None, None, "Protected", None, true).unwrap();
        create(&root, None, None, "Unprotected", None, false).unwrap();
        resolve(&root, &first.id, "Resolved").unwrap();
        let second = create(&root, None, None, "Still open", None, true).unwrap();
        let result = list_open_protected(&root).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, second.id);
    }
}
