use crate::error::AppError;
use crate::project::{paths, repository as project_repository};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceKind {
    AssetVersion,
    WorkflowRun,
    Generation,
    QaRun,
    RepairVersion,
    Scene,
    Shot,
    CinemaCompile,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceNode {
    pub id: String,
    pub kind: ProvenanceKind,
    pub label: String,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceEdge {
    pub from: String,
    pub to: String,
    pub relation: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceGraph {
    pub target_id: String,
    pub nodes: Vec<ProvenanceNode>,
    pub edges: Vec<ProvenanceEdge>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
struct NodeKey {
    kind: String,
    id: String,
}

pub fn get_provenance_graph(
    root: &Path,
    target_kind: &str,
    target_id: &str,
) -> Result<ProvenanceGraph, AppError> {
    let manifest = paths::read_manifest(root)?;
    let conn = crate::db::open_existing_connection(&root.join("project.db"))?;
    if project_repository::read_project(&conn)?.id != manifest.project_id {
        return Err(AppError::ProjectIdentityMismatch);
    }

    let mut nodes_map: HashMap<NodeKey, ProvenanceNode> = HashMap::new();
    let mut edges_vec: Vec<ProvenanceEdge> = Vec::new();
    let mut visited: HashSet<NodeKey> = HashSet::new();
    let mut queue: VecDeque<(String, String)> = VecDeque::new();

    let _start_kind = parse_kind(target_kind)?;
    queue.push_back((target_kind.to_string(), target_id.to_string()));

    while let Some((kind_str, id)) = queue.pop_front() {
        let key = NodeKey {
            kind: kind_str.clone(),
            id: id.clone(),
        };

        if visited.contains(&key) {
            continue;
        }
        visited.insert(key);

        // Add the current node
        let node = build_node(&conn, &kind_str, &id)?;
        nodes_map.insert(
            NodeKey {
                kind: kind_str.clone(),
                id: id.clone(),
            },
            node,
        );

        // Traverse backwards (dependencies)
        traverse_backwards(&conn, &kind_str, &id, &mut nodes_map, &mut edges_vec, &mut queue)?;

        // Traverse forwards (dependents)
        traverse_forwards(&conn, &kind_str, &id, &mut nodes_map, &mut edges_vec, &mut queue)?;
    }

    let mut nodes: Vec<ProvenanceNode> = nodes_map.into_values().collect();
    nodes.sort_by(|a, b| {
        a.timestamp
            .as_ref()
            .cmp(&b.timestamp.as_ref())
            .then_with(|| a.id.cmp(&b.id))
    });

    Ok(ProvenanceGraph {
        target_id: target_id.to_string(),
        nodes,
        edges: edges_vec,
    })
}

fn parse_kind(kind_str: &str) -> Result<ProvenanceKind, AppError> {
    match kind_str {
        "asset_version" => Ok(ProvenanceKind::AssetVersion),
        "workflow_run" => Ok(ProvenanceKind::WorkflowRun),
        "generation" => Ok(ProvenanceKind::Generation),
        "qa_run" => Ok(ProvenanceKind::QaRun),
        "repair_version" => Ok(ProvenanceKind::RepairVersion),
        "scene" => Ok(ProvenanceKind::Scene),
        "shot" => Ok(ProvenanceKind::Shot),
        "cinema_compile" => Ok(ProvenanceKind::CinemaCompile),
        _ => Err(AppError::InvalidProjectDirectory),
    }
}

fn build_node(
    conn: &Connection,
    kind_str: &str,
    id: &str,
) -> Result<ProvenanceNode, AppError> {
    let kind = parse_kind(kind_str)?;

    let (label, timestamp) = match kind_str {
        "asset_version" => {
            let mut stmt = conn
                .prepare("SELECT a.label, av.created_at FROM asset_versions av JOIN assets a ON av.asset_id = a.id WHERE av.id = ?1")
                .map_err(|e| AppError::Database(e.to_string()))?;
            stmt.query_row([id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
                .map_err(|e| AppError::Database(e.to_string()))?
        }
        "workflow_run" => {
            let mut stmt = conn
                .prepare("SELECT workflow_definition_id, created_at FROM workflow_runs WHERE id = ?1")
                .map_err(|e| AppError::Database(e.to_string()))?;
            let (def_id, ts) =
                stmt.query_row([id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
                    .map_err(|e| AppError::Database(e.to_string()))?;
            (format!("Workflow {}", def_id), ts)
        }
        "generation" => {
            let mut stmt = conn
                .prepare("SELECT created_at FROM generated_artifacts WHERE id = ?1")
                .map_err(|e| AppError::Database(e.to_string()))?;
            let ts = stmt
                .query_row([id], |row| row.get::<_, String>(0))
                .map_err(|e| AppError::Database(e.to_string()))?;
            ("Generated artifact".to_string(), ts)
        }
        "qa_run" => {
            let mut stmt = conn
                .prepare("SELECT asset_version_id, created_at FROM qa_runs WHERE id = ?1")
                .map_err(|e| AppError::Database(e.to_string()))?;
            let (av_id, ts) = stmt
                .query_row([id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
                .map_err(|e| AppError::Database(e.to_string()))?;
            (format!("QA of {}", av_id), ts)
        }
        "repair_version" => {
            let mut stmt = conn
                .prepare("SELECT qa_run_id, created_at FROM qa_repairs WHERE id = ?1")
                .map_err(|e| AppError::Database(e.to_string()))?;
            let (qa_id, ts) = stmt
                .query_row([id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
                .map_err(|e| AppError::Database(e.to_string()))?;
            (format!("Repair from QA {}", qa_id), ts)
        }
        "scene" => {
            let mut stmt = conn
                .prepare("SELECT intent, created_at FROM scenes WHERE id = ?1")
                .map_err(|e| AppError::Database(e.to_string()))?;
            stmt.query_row([id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
                .map_err(|e| AppError::Database(e.to_string()))?
        }
        "shot" => {
            let mut stmt = conn
                .prepare("SELECT intent, created_at FROM shots WHERE id = ?1")
                .map_err(|e| AppError::Database(e.to_string()))?;
            stmt.query_row([id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
                .map_err(|e| AppError::Database(e.to_string()))?
        }
        "cinema_compile" => {
            let mut stmt = conn
                .prepare("SELECT scene_id, created_at FROM cinema_compilations WHERE id = ?1")
                .map_err(|e| AppError::Database(e.to_string()))?;
            let (scene_id, ts) = stmt
                .query_row([id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
                .map_err(|e| AppError::Database(e.to_string()))?;
            (format!("Cinema from {}", scene_id), ts)
        }
        _ => ("Unknown".to_string(), String::new()),
    };

    Ok(ProvenanceNode {
        id: id.to_string(),
        kind,
        label,
        timestamp: if timestamp.is_empty() {
            None
        } else {
            Some(timestamp)
        },
    })
}

fn traverse_backwards(
    conn: &Connection,
    kind_str: &str,
    id: &str,
    nodes_map: &mut HashMap<NodeKey, ProvenanceNode>,
    edges_vec: &mut Vec<ProvenanceEdge>,
    queue: &mut VecDeque<(String, String)>,
) -> Result<(), AppError> {
    match kind_str {
        "asset_version" => {
            // Asset Version -> Generation -> Workflow Run
            let mut stmt = conn
                .prepare(
                    "SELECT DISTINCT al.artifact_id FROM artifact_lineage al
                     JOIN generated_artifact_sources gs ON gs.artifact_id = al.artifact_id
                     WHERE gs.asset_version_id = ?1",
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
            for row in stmt
                .query_map([id], |r| r.get::<_, String>(0))
                .map_err(|e| AppError::Database(e.to_string()))?
            {
                let gen_id = row.map_err(|e| AppError::Database(e.to_string()))?;
                add_node_if_missing(conn, "generation", &gen_id, nodes_map, queue)?;
                edges_vec.push(ProvenanceEdge {
                    from: gen_id,
                    to: id.to_string(),
                    relation: "OUTPUT_OF".to_string(),
                });
            }

            // Check for repair parent
            let mut stmt = conn
                .prepare(
                    "SELECT parent_asset_version_id FROM qa_repairs WHERE id = ?1 AND parent_asset_version_id IS NOT NULL",
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
            if let Some(parent_id) = stmt
                .query_row([id], |r| r.get::<_, String>(0))
                .optional()
                .map_err(|e| AppError::Database(e.to_string()))?
            {
                add_node_if_missing(conn, "asset_version", &parent_id, nodes_map, queue)?;
                edges_vec.push(ProvenanceEdge {
                    from: id.to_string(),
                    to: parent_id,
                    relation: "REPAIRS".to_string(),
                });
            }

            // Check for QA runs
            let mut stmt = conn
                .prepare("SELECT id FROM qa_runs WHERE asset_version_id = ?1")
                .map_err(|e| AppError::Database(e.to_string()))?;
            for row in stmt
                .query_map([id], |r| r.get::<_, String>(0))
                .map_err(|e| AppError::Database(e.to_string()))?
            {
                let qa_id = row.map_err(|e| AppError::Database(e.to_string()))?;
                add_node_if_missing(conn, "qa_run", &qa_id, nodes_map, queue)?;
                edges_vec.push(ProvenanceEdge {
                    from: qa_id,
                    to: id.to_string(),
                    relation: "QA_OF".to_string(),
                });
            }
        }
        "generation" => {
            // Generation -> Workflow Run
            let mut stmt = conn
                .prepare("SELECT workflow_run_id FROM artifact_lineage WHERE artifact_id = ?1")
                .map_err(|e| AppError::Database(e.to_string()))?;
            if let Some(wr_id) = stmt
                .query_row([id], |r| r.get::<_, String>(0))
                .optional()
                .map_err(|e| AppError::Database(e.to_string()))?
            {
                add_node_if_missing(conn, "workflow_run", &wr_id, nodes_map, queue)?;
                edges_vec.push(ProvenanceEdge {
                    from: wr_id,
                    to: id.to_string(),
                    relation: "INPUT_TO".to_string(),
                });
            }
        }
        "workflow_run" => {
            // Workflow Run -> Canon Revision (via context)
            let mut stmt = conn
                .prepare(
                    "SELECT ce.id FROM workflow_runs wr
                     JOIN canon_entities ce ON wr.id = ce.id
                     WHERE wr.id = ?1 LIMIT 1",
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
            if let Some(canon_id) = stmt
                .query_row([id], |r| r.get::<_, String>(0))
                .optional()
                .map_err(|e| AppError::Database(e.to_string()))?
            {
                // Use the canon_id as reference
                edges_vec.push(ProvenanceEdge {
                    from: id.to_string(),
                    to: canon_id,
                    relation: "USES_IDENTITY".to_string(),
                });
            }
        }
        "qa_run" => {
            // QA Run -> Asset Version
            let mut stmt = conn
                .prepare("SELECT asset_version_id FROM qa_runs WHERE id = ?1")
                .map_err(|e| AppError::Database(e.to_string()))?;
            if let Some(av_id) = stmt
                .query_row([id], |r| r.get::<_, String>(0))
                .optional()
                .map_err(|e| AppError::Database(e.to_string()))?
            {
                add_node_if_missing(conn, "asset_version", &av_id, nodes_map, queue)?;
                edges_vec.push(ProvenanceEdge {
                    from: id.to_string(),
                    to: av_id,
                    relation: "QA_OF".to_string(),
                });
            }
        }
        "repair_version" => {
            // Repair -> QA Run
            let mut stmt = conn
                .prepare("SELECT qa_run_id FROM qa_repairs WHERE id = ?1")
                .map_err(|e| AppError::Database(e.to_string()))?;
            if let Some(qa_id) = stmt
                .query_row([id], |r| r.get::<_, String>(0))
                .optional()
                .map_err(|e| AppError::Database(e.to_string()))?
            {
                add_node_if_missing(conn, "qa_run", &qa_id, nodes_map, queue)?;
                edges_vec.push(ProvenanceEdge {
                    from: id.to_string(),
                    to: qa_id,
                    relation: "DERIVED_FROM".to_string(),
                });
            }

            // Repair -> Parent Asset Version
            let mut stmt = conn
                .prepare("SELECT parent_asset_version_id FROM qa_repairs WHERE id = ?1")
                .map_err(|e| AppError::Database(e.to_string()))?;
            if let Some(parent_id) = stmt
                .query_row([id], |r| r.get::<_, String>(0))
                .optional()
                .map_err(|e| AppError::Database(e.to_string()))?
            {
                add_node_if_missing(conn, "asset_version", &parent_id, nodes_map, queue)?;
                edges_vec.push(ProvenanceEdge {
                    from: id.to_string(),
                    to: parent_id,
                    relation: "REPAIRS".to_string(),
                });
            }
        }
        "scene" => {
            // Scene -> Character Look Version
            let mut stmt = conn
                .prepare(
                    "SELECT character_look_asset_version_id FROM scene_characters WHERE scene_id = ?1",
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
            for row in stmt
                .query_map([id], |r| r.get::<_, String>(0))
                .map_err(|e| AppError::Database(e.to_string()))?
            {
                let av_id = row.map_err(|e| AppError::Database(e.to_string()))?;
                add_node_if_missing(conn, "asset_version", &av_id, nodes_map, queue)?;
                edges_vec.push(ProvenanceEdge {
                    from: id.to_string(),
                    to: av_id,
                    relation: "USES_LOOK".to_string(),
                });
            }

            // Scene -> World Version
            let mut stmt = conn
                .prepare("SELECT world_asset_version_id FROM scenes WHERE id = ?1")
                .map_err(|e| AppError::Database(e.to_string()))?;
            if let Some(world_id) = stmt
                .query_row([id], |r| r.get::<_, String>(0))
                .optional()
                .map_err(|e| AppError::Database(e.to_string()))?
            {
                add_node_if_missing(conn, "asset_version", &world_id, nodes_map, queue)?;
                edges_vec.push(ProvenanceEdge {
                    from: id.to_string(),
                    to: world_id,
                    relation: "USES_WORLD".to_string(),
                });
            }

            // Scene -> Props
            let mut stmt = conn
                .prepare("SELECT prop_asset_version_id FROM scene_props WHERE scene_id = ?1")
                .map_err(|e| AppError::Database(e.to_string()))?;
            for row in stmt
                .query_map([id], |r| r.get::<_, String>(0))
                .map_err(|e| AppError::Database(e.to_string()))?
            {
                let prop_id = row.map_err(|e| AppError::Database(e.to_string()))?;
                add_node_if_missing(conn, "asset_version", &prop_id, nodes_map, queue)?;
                edges_vec.push(ProvenanceEdge {
                    from: id.to_string(),
                    to: prop_id,
                    relation: "USES_PROP".to_string(),
                });
            }
        }
        "shot" => {
            // Shot -> Scene
            let mut stmt = conn
                .prepare("SELECT scene_id FROM shots WHERE id = ?1")
                .map_err(|e| AppError::Database(e.to_string()))?;
            if let Some(scene_id) = stmt
                .query_row([id], |r| r.get::<_, String>(0))
                .optional()
                .map_err(|e| AppError::Database(e.to_string()))?
            {
                add_node_if_missing(conn, "scene", &scene_id, nodes_map, queue)?;
                edges_vec.push(ProvenanceEdge {
                    from: id.to_string(),
                    to: scene_id,
                    relation: "KEYFRAME_FOR".to_string(),
                });
            }
        }
        "cinema_compile" => {
            // Cinema Compile -> Scene
            let mut stmt = conn
                .prepare("SELECT scene_id FROM cinema_compilations WHERE id = ?1")
                .map_err(|e| AppError::Database(e.to_string()))?;
            if let Some(scene_id) = stmt
                .query_row([id], |r| r.get::<_, String>(0))
                .optional()
                .map_err(|e| AppError::Database(e.to_string()))?
            {
                add_node_if_missing(conn, "scene", &scene_id, nodes_map, queue)?;
                edges_vec.push(ProvenanceEdge {
                    from: id.to_string(),
                    to: scene_id,
                    relation: "COMPILED_FROM".to_string(),
                });
            }
        }
        _ => {}
    }

    Ok(())
}

fn traverse_forwards(
    conn: &Connection,
    kind_str: &str,
    id: &str,
    nodes_map: &mut HashMap<NodeKey, ProvenanceNode>,
    edges_vec: &mut Vec<ProvenanceEdge>,
    queue: &mut VecDeque<(String, String)>,
) -> Result<(), AppError> {
    match kind_str {
        "scene" => {
            // Scene -> Shots
            let mut stmt = conn
                .prepare("SELECT id FROM shots WHERE scene_id = ?1")
                .map_err(|e| AppError::Database(e.to_string()))?;
            for row in stmt
                .query_map([id], |r| r.get::<_, String>(0))
                .map_err(|e| AppError::Database(e.to_string()))?
            {
                let shot_id = row.map_err(|e| AppError::Database(e.to_string()))?;
                add_node_if_missing(conn, "shot", &shot_id, nodes_map, queue)?;
                edges_vec.push(ProvenanceEdge {
                    from: shot_id,
                    to: id.to_string(),
                    relation: "KEYFRAME_FOR".to_string(),
                });
            }

            // Scene -> Cinema Compilations
            let mut stmt = conn
                .prepare("SELECT id FROM cinema_compilations WHERE scene_id = ?1")
                .map_err(|e| AppError::Database(e.to_string()))?;
            for row in stmt
                .query_map([id], |r| r.get::<_, String>(0))
                .map_err(|e| AppError::Database(e.to_string()))?
            {
                let cinema_id = row.map_err(|e| AppError::Database(e.to_string()))?;
                add_node_if_missing(conn, "cinema_compile", &cinema_id, nodes_map, queue)?;
                edges_vec.push(ProvenanceEdge {
                    from: cinema_id,
                    to: id.to_string(),
                    relation: "COMPILED_FROM".to_string(),
                });
            }
        }
        _ => {}
    }

    Ok(())
}

fn add_node_if_missing(
    conn: &Connection,
    kind_str: &str,
    id: &str,
    nodes_map: &mut HashMap<NodeKey, ProvenanceNode>,
    queue: &mut VecDeque<(String, String)>,
) -> Result<(), AppError> {
    let key = NodeKey {
        kind: kind_str.to_string(),
        id: id.to_string(),
    };

    if !nodes_map.contains_key(&key) {
        let node = build_node(conn, kind_str, id)?;
        nodes_map.insert(key.clone(), node);
        queue.push_back((kind_str.to_string(), id.to_string()));
    }

    Ok(())
}
