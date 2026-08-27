use serde::{Deserialize, Serialize};

/// Row shape stored in the project's own `projects` table.
#[derive(Debug, Clone)]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub schema_version: u32,
}

/// Summary returned to the frontend after a project is created or opened.
/// Field names are serialized as camelCase to match
/// `packages/domain/src/project.ts`'s `ProjectSummary`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub schema_version: u32,
    pub created_at: String,
    pub updated_at: String,
}
