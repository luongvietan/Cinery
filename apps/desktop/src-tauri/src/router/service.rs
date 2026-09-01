use crate::db;
use crate::error::AppError;
use crate::project::repository::read_project;
use crate::providers::credential_store::{
    credential_account, CredentialStore, KeyringCredentialStore,
};
use crate::providers::http::{HttpBody, HttpExecutor, HttpRequest, UreqExecutor};
use crate::router::model::{RouteProductionIntentResult, RoutedOperation};
use crate::skills::model::{AssetType, Prerequisite, SkillOperation};
use crate::skills::registry::SkillRegistry;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::time::Duration;

/// Deterministic production intent router (master plan §13).
///
/// The router scores intent text against each operation's intent examples,
/// then validates prerequisites with deterministic project queries. The
/// optional LLM classifier (the project's configured `purpose = llm`
/// provider) may only *propose* an operation id; the id is re-validated
/// against the registry, the score is capped below deterministic matches,
/// and prerequisites are always evaluated by code. Routing never executes
/// anything: the frontend still has to create the workflow run (and its
/// approval gate) itself.
///
/// The router is read-only: it opens the project database, runs SELECTs,
/// and never mutates project state.
pub struct ProductionRouter;

impl ProductionRouter {
    pub fn route(project_root: &Path, text: &str) -> Result<RouteProductionIntentResult, AppError> {
        let registry = SkillRegistry::builtin()?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;

        let llm_suggestion = suggest_operation_via_llm(project_root, text).ok().flatten();
        let mut candidates = Vec::new();
        for skill in registry.list() {
            for operation in &skill.operations {
                let deterministic = intent_score(text, operation);
                // The LLM only proposes; an operation still needs either a
                // deterministic match or the LLM's explicit suggestion, and
                // code always validates prerequisites. LLM-only suggestions
                // score 50 -- below any real deterministic match.
                let score = if deterministic > 0 {
                    deterministic
                } else if llm_suggestion.as_deref() == Some(operation.id.as_str()) {
                    50
                } else {
                    0
                };
                if score == 0 {
                    continue;
                }
                let (prerequisite_passed, prerequisite_blockers) =
                    evaluate_feasibility(&conn, &operation.prerequisites)?;
                candidates.push(RoutedOperation {
                    skill_id: skill.id.clone(),
                    skill_version: skill.version.clone(),
                    operation_id: operation.id.clone(),
                    operation_name: operation.name.clone(),
                    score,
                    prerequisite_passed,
                    prerequisite_blockers,
                });
            }
        }
        candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.score));

        let suggested = candidates.first().cloned();
        let matched = suggested.is_some();
        Ok(RouteProductionIntentResult {
            matched,
            suggested,
            candidates,
        })
    }
}

const CLASSIFIER_TIMEOUT: Duration = Duration::from_secs(30);

/// Optionally asks the project's configured text AI service (custom provider
/// with purpose `llm`) to classify the intent into an operation id. Never
/// trusted blindly: the caller re-validates the returned id against the
/// registry, and the suggestion can never outrank a deterministic match.
/// Returns `Ok(None)` when no LLM service is configured or the call fails,
/// so routing always degrades to the deterministic matcher.
fn suggest_operation_via_llm(project_root: &Path, text: &str) -> Result<Option<String>, AppError> {
    let definition = match first_llm_provider(project_root)? {
        Some(definition) => definition,
        None => return Ok(None),
    };
    let conn = db::open_existing_connection(&project_root.join("project.db"))?;
    let project_id = read_project(&conn)?.id;
    drop(conn);
    let account = credential_account(&project_id, &definition.provider_id);
    let store = KeyringCredentialStore::new();
    let Ok(Some(token)) = store.get_secret(&account) else {
        return Ok(None);
    };
    if token.trim().is_empty() {
        return Ok(None);
    }
    let endpoint = format!(
        "{}/chat/completions",
        definition.base_url.trim_end_matches('/')
    );
    let transport = UreqExecutor::new(CLASSIFIER_TIMEOUT);
    classify_with_transport(&transport, &endpoint, &token, text)
        .map_err(|_| AppError::ProviderExecution("the routing classifier request failed".into()))
}

/// The classifier core, transport-injected for deterministic tests. The
/// system prompt lists only registered operation ids, and any returned id
/// that is not in the registry is dropped before it can reach scoring.
fn classify_with_transport(
    transport: &dyn HttpExecutor,
    endpoint: &str,
    token: &str,
    text: &str,
) -> Result<Option<String>, AppError> {
    let registry = SkillRegistry::builtin()?;
    let operation_ids: Vec<String> = registry
        .list()
        .iter()
        .flat_map(|skill| {
            skill
                .operations
                .iter()
                .map(|operation| operation.id.clone())
        })
        .collect();
    let body = serde_json::json!({
        "model": "gpt-4o-mini",
        "temperature": 0,
        "messages": [
            {
                "role": "system",
                "content": format!(
                    "You route film production intents to one of these operation ids: {}. Reply with ONLY the operation id, or the token NONE if none fit.",
                    operation_ids.join(", ")
                )
            },
            { "role": "user", "content": text }
        ]
    });
    let request = HttpRequest {
        method: "POST".into(),
        url: endpoint.into(),
        headers: vec![("Authorization".into(), format!("Bearer {token}"))],
        body: HttpBody::Json(body),
        max_response_bytes: 10 * 1024 * 1024,
    };
    let response = transport.execute(request).map_err(|diagnostic| {
        AppError::ProviderExecution(super::super::providers::error::redact_secret(&format!(
            "the routing classifier request failed: {diagnostic}"
        )))
    })?;
    if !response.is_success() {
        return Ok(None);
    }
    let document = response.json().map_err(|_| {
        AppError::ProviderExecution("the routing classifier returned an unreadable response".into())
    })?;
    let suggestion = document
        .pointer("/choices/0/message/content")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| *value != "NONE" && !value.is_empty())
        .map(str::to_string);
    // Code validates the proposal: unknown ids are dropped.
    Ok(suggestion.filter(|id| operation_ids.iter().any(|candidate| candidate == id)))
}

fn first_llm_provider(
    project_root: &Path,
) -> Result<Option<crate::providers::model::CustomProviderDefinition>, AppError> {
    let conn = db::open_existing_connection(&project_root.join("project.db"))?;
    let definitions = crate::providers::repository::list_custom_providers(&conn)?;
    Ok(definitions.into_iter().find(|definition| {
        definition.purpose == crate::providers::model::CustomProviderPurpose::Llm
    }))
}

const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "this", "that", "these", "those", "in", "on", "at", "for", "to", "of", "and",
    "or", "is", "are", "be", "with", "new", "only", "failed", "run",
];

fn intent_score(text: &str, operation: &SkillOperation) -> i32 {
    let haystack = text.to_lowercase();
    let best = operation
        .intent_examples
        .iter()
        .map(|example| {
            let example = example.to_lowercase();
            let words: Vec<&str> = example
                .split_whitespace()
                .filter(|word| !STOP_WORDS.contains(word))
                .collect();
            let matched_words = words
                .iter()
                .filter(|word| haystack.contains(**word))
                .count();
            if words.is_empty() {
                0
            } else {
                (matched_words * 100 / words.len()) as i32
            }
        })
        .max()
        .unwrap_or(0);
    // Require at least half of the meaningful example words to match.
    if best >= 50 {
        best
    } else {
        0
    }
}

/// Feasibility check without a concrete workflow input: the router only
/// answers "could this operation run at all in this project right now?".
/// The authoritative per-input prerequisite evaluation still happens in
/// WorkflowRuntime::create_run when a run is actually launched.
fn evaluate_feasibility(
    conn: &Connection,
    prerequisites: &[Prerequisite],
) -> Result<(bool, Vec<String>), AppError> {
    let mut blockers = Vec::new();
    for prerequisite in prerequisites {
        let satisfied = match prerequisite {
            Prerequisite::CanonEntityExists { entity_type, .. } => {
                let found: Option<String> = conn
                    .query_row(
                        "SELECT id FROM canon_entities WHERE type = ?1 LIMIT 1",
                        params![entity_type.as_str()],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(db_error)?;
                found.is_some()
            }
            Prerequisite::CanonSectionLocked { .. } => {
                let found: Option<String> = conn
                    .query_row(
                        "SELECT s.id FROM canon_sections s WHERE s.status = 'locked' LIMIT 1",
                        [],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(db_error)?;
                found.is_some()
            }
            Prerequisite::CanonicalAssetExists { asset_type, .. } => {
                let asset_type_str = asset_type.as_str();
                let found: Option<String> = conn
                    .query_row(
                        "SELECT a.id FROM assets a JOIN asset_versions v ON v.id = a.canonical_version_id WHERE a.type = ?1 AND v.status = 'canonical' LIMIT 1",
                        params![asset_type_str],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(db_error)?;
                found.is_some()
            }
            Prerequisite::AssetVersionStatus { .. } => true,
        };
        if !satisfied {
            blockers.push(describe_prerequisite(prerequisite));
        }
    }
    Ok((blockers.is_empty(), blockers))
}

fn describe_prerequisite(prerequisite: &Prerequisite) -> String {
    match prerequisite {
        Prerequisite::CanonEntityExists { entity_type, .. } => {
            format!(
                "Create at least one {} entity in Canon",
                entity_type.as_str()
            )
        }
        Prerequisite::CanonSectionLocked { .. } => "Lock the relevant Canon section".to_string(),
        Prerequisite::CanonicalAssetExists { asset_type, .. } => match asset_type {
            AssetType::FaceLock => "Promote a canonical face lock first".to_string(),
            AssetType::Outfit => "Promote a canonical outfit first".to_string(),
            AssetType::CharacterSheet => "Promote a canonical character sheet first".to_string(),
            other => format!("Promote a canonical {} first", other.as_str()),
        },
        Prerequisite::AssetVersionStatus { status, .. } => {
            format!("Prepare an asset version with status {}", status.as_str())
        }
    }
}

fn db_error(error: rusqlite::Error) -> AppError {
    AppError::Database(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::service::ProjectService;
    use crate::providers::http::{HttpResponse, TransportFailure};
    use tempfile::tempdir;

    #[test]
    fn face_lock_intent_routes_to_face_lock_operation() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "Red Door").unwrap();
        let result =
            ProductionRouter::route(&root, "Create a face lock for this character").unwrap();
        assert!(result.matched);
        assert_eq!(
            result.suggested.unwrap().operation_id,
            "character.create_face_lock"
        );
    }

    #[test]
    fn wardrobe_intent_routes_to_outfit_with_prerequisite_blockers() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "Red Door").unwrap();
        let result = ProductionRouter::route(&root, "Put this character in a new outfit").unwrap();
        let suggestion = result.suggested.unwrap();
        assert_eq!(suggestion.operation_id, "character.create_outfit");
        // No canonical face in this fresh project -> blocked.
        assert!(!suggestion.prerequisite_passed);
        assert!(suggestion
            .prerequisite_blockers
            .iter()
            .any(|blocker| blocker.contains("canonical face lock")));
    }

    #[test]
    fn scene_video_intent_routes_to_scene_generate_video() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "Red Door").unwrap();
        let result = ProductionRouter::route(&root, "Turn this scene into a video").unwrap();
        assert!(result.matched);
        assert_eq!(
            result.suggested.unwrap().operation_id,
            "scene.generate_video"
        );
    }

    #[test]
    fn unrelated_text_produces_no_match() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "Red Door").unwrap();
        let result = ProductionRouter::route(&root, "hello world this is unrelated").unwrap();
        assert!(!result.matched);
        assert!(result.suggested.is_none());
    }

    struct FixtureClassifierExecutor {
        content: &'static str,
    }

    impl HttpExecutor for FixtureClassifierExecutor {
        fn execute(&self, _request: HttpRequest) -> Result<HttpResponse, TransportFailure> {
            Ok(HttpResponse {
                status: 200,
                body: serde_json::json!({
                    "choices": [{ "message": { "content": self.content } }]
                })
                .to_string()
                .into_bytes(),
                content_type: Some("application/json".into()),
                headers: Vec::new(),
            })
        }
    }

    #[test]
    fn classifier_accepts_a_known_operation_id_and_drops_unknown_ones() {
        let transport = FixtureClassifierExecutor {
            content: "character.create_outfit",
        };
        let suggestion = classify_with_transport(
            &transport,
            "https://example.invalid/v1/chat/completions",
            "token",
            "dress mara",
        )
        .unwrap();
        assert_eq!(suggestion.as_deref(), Some("character.create_outfit"));

        let unknown = FixtureClassifierExecutor {
            content: "made.up.operation",
        };
        assert_eq!(
            classify_with_transport(
                &unknown,
                "https://example.invalid/v1/chat/completions",
                "token",
                "dress mara"
            )
            .unwrap(),
            None
        );

        let none = FixtureClassifierExecutor { content: "NONE" };
        assert_eq!(
            classify_with_transport(
                &none,
                "https://example.invalid/v1/chat/completions",
                "token",
                "dress mara"
            )
            .unwrap(),
            None
        );
    }
}
