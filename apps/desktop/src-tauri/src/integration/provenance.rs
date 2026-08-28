use crate::error::AppError;
use crate::project::{paths, repository as project_repository};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceKind { AssetVersion, WorkflowRun, Generation, QaRun, RepairVersion, Scene, Shot, CinemaCompile }

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceNode { pub id: String, pub kind: ProvenanceKind, pub label: String, pub timestamp: Option<String> }

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceEdge { pub from: String, pub to: String, pub relation: String }

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceGraph { pub target_id: String, pub nodes: Vec<ProvenanceNode>, pub edges: Vec<ProvenanceEdge> }

pub fn get_provenance_graph(root: &Path, target_kind: &str, target_id: &str) -> Result<ProvenanceGraph, AppError> {
    let manifest = paths::read_manifest(root)?;
    let conn = crate::db::open_existing_connection(&root.join("project.db"))?;
    if project_repository::read_project(&conn)?.id != manifest.project_id { return Err(AppError::ProjectIdentityMismatch); }
    let mut nodes = Vec::new(); let mut edges = Vec::new();
    let kind = match target_kind { "asset_version" => ProvenanceKind::AssetVersion, "workflow_run" => ProvenanceKind::WorkflowRun, "generation" => ProvenanceKind::Generation, "qa_run" => ProvenanceKind::QaRun, "repair_version" => ProvenanceKind::RepairVersion, "scene" => ProvenanceKind::Scene, "shot" => ProvenanceKind::Shot, "cinema_compile" => ProvenanceKind::CinemaCompile, _ => return Err(AppError::InvalidProjectDirectory) };
    nodes.push(ProvenanceNode { id: target_id.into(), kind, label: format!("{target_kind} {target_id}"), timestamp: None });
    if target_kind == "scene" {
        let mut stmt = conn.prepare("SELECT id, intent, created_at FROM shots WHERE scene_id = ?1 ORDER BY ordering").map_err(|e| AppError::Database(e.to_string()))?;
        for row in stmt.query_map([target_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))).map_err(|e| AppError::Database(e.to_string()))? {
            let (id, label, timestamp) = row.map_err(|e| AppError::Database(e.to_string()))?;
            nodes.push(ProvenanceNode { id: id.clone(), kind: ProvenanceKind::Shot, label, timestamp: Some(timestamp) }); edges.push(ProvenanceEdge { from: id, to: target_id.into(), relation: "KEYFRAME_FOR".into() });
        }
        let mut stmt = conn.prepare("SELECT id, created_at FROM cinema_compilations WHERE scene_id = ?1 ORDER BY created_at").map_err(|e| AppError::Database(e.to_string()))?;
        for row in stmt.query_map([target_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))).map_err(|e| AppError::Database(e.to_string()))? { let (id, timestamp) = row.map_err(|e| AppError::Database(e.to_string()))?; nodes.push(ProvenanceNode { id: id.clone(), kind: ProvenanceKind::CinemaCompile, label: "Cinema compilation".into(), timestamp: Some(timestamp) }); edges.push(ProvenanceEdge { from: id, to: target_id.into(), relation: "COMPILED_FROM".into() }); }
    } else if target_kind == "asset_version" {
        let mut stmt = conn.prepare("SELECT DISTINCT al.artifact_id, al.created_at FROM artifact_lineage al JOIN generated_artifact_sources gs ON gs.artifact_id = al.artifact_id WHERE gs.asset_version_id = ?1").map_err(|e| AppError::Database(e.to_string()))?;
        for row in stmt.query_map([target_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))).map_err(|e| AppError::Database(e.to_string()))? { let (id, timestamp) = row.map_err(|e| AppError::Database(e.to_string()))?; nodes.push(ProvenanceNode { id: id.clone(), kind: ProvenanceKind::Generation, label: "Generated artifact".into(), timestamp: Some(timestamp) }); edges.push(ProvenanceEdge { from: id, to: target_id.into(), relation: "OUTPUT_OF".into() }); }
    }
    Ok(ProvenanceGraph { target_id: target_id.into(), nodes, edges })
}
