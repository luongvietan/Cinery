use super::credential_store::{credential_account, CredentialStore, KeyringCredentialStore};
use super::http::{HttpBody, HttpExecutor, HttpRequest, UreqExecutor};
use super::service::ProviderService;
use crate::db;
use crate::error::AppError;
use crate::project::repository::read_project;
use std::path::Path;
use std::time::Duration;

/// The visual-spec fields the character forms ask the user to fill, in the
/// order the face-lock compiler consumes them.
pub const VISUAL_SPEC_FIELDS: [&str; 9] = [
    "head", "eyes", "brows", "nose", "lips", "skin", "hair", "build", "expression",
];

const SUGGEST_TIMEOUT: Duration = Duration::from_secs(45);

/// Suggests face-lock visual spec fields for a character using the project's
/// configured LLM service (a custom provider with purpose `llm`). The LLM
/// proposes; nothing here mutates canon — the user edits and approves the
/// suggestion before any run is created.
///
/// Prompt guidance follows the reference cinema/character skills: describe
/// measurable muscle and geometry, not mood words, and include a "never"
/// clause per trait where it helps the model stay on-model.
pub fn suggest_visual_spec(
    project_root: &Path,
    credentials: Option<&dyn CredentialStore>,
    character_name: &str,
    notes: &str,
) -> Result<serde_json::Value, AppError> {
    let definition = first_llm_provider(project_root)?
        .ok_or_else(|| {
            AppError::ProviderConfiguration(
                "No text AI service is connected. Open AI Services and add a service with the Text (LLM) purpose to get suggestions.".into(),
            )
        })?;
    let model = ProviderService::default_model_for(project_root, &definition.provider_id)?
        .or_else(|| definition.models.first().map(|model| model.id.clone()))
        .ok_or_else(|| {
            AppError::ProviderConfiguration(format!(
                "The text service {} has no model configured.",
                definition.provider_id
            ))
        })?;

    let store_owned;
    let store: &dyn CredentialStore = match credentials {
        Some(store) => store,
        None => {
            store_owned = KeyringCredentialStore::new();
            &store_owned
        }
    };
    let conn = db::open_existing_connection(&project_root.join("project.db"))?;
    let project_id = read_project(&conn)?.id;
    drop(conn);
    let account = credential_account(&project_id, &definition.provider_id);
    let mut token = store
        .get_secret(&account)
        .map_err(|_| AppError::ProviderConfiguration("reading the credential failed".into()))?
        .unwrap_or_default();
    if token.trim().is_empty() {
        for header in &definition.headers {
            if header.name.eq_ignore_ascii_case("authorization") {
                token = header.value.clone().unwrap_or_default();
                break;
            }
        }
    }
    if token.trim().is_empty() {
        return Err(AppError::ProviderConfiguration(format!(
            "The text service {} has no API key. Add the key in AI Services.",
            definition.provider_id
        )));
    }

    let endpoint = format!(
        "{}/chat/completions",
        definition.base_url.trim_end_matches('/')
    );
    let body = serde_json::json!({
        "model": model,
        "temperature": 0.7,
        "response_format": { "type": "json_object" },
        "messages": [
            {
                "role": "system",
                "content": SUGGESTION_SYSTEM_PROMPT,
            },
            {
                "role": "user",
                "content": format!(
                    "Character name: {character_name}\nExtra context from the user: {}",
                    if notes.trim().is_empty() { "(none)" } else { notes }
                ),
            },
        ],
    });
    let request = HttpRequest {
        method: "POST".into(),
        url: endpoint,
        headers: vec![("Authorization".into(), format!("Bearer {token}"))],
        body: HttpBody::Json(body),
        max_response_bytes: 10 * 1024 * 1024,
    };
    let response = UreqExecutor::new(SUGGEST_TIMEOUT).execute(request)
        .map_err(|diagnostic| {
            AppError::ProviderExecution(super::error::redact_secret(&format!(
                "the text service request failed: {diagnostic}"
            )))
        })?;
    if !response.is_success() {
        return Err(AppError::ProviderExecution(format!(
            "the text service returned HTTP {}",
            response.status
        )));
    }
    let document = response.json().map_err(|error| {
        AppError::ProviderExecution(format!(
            "the text service returned an unreadable response: {error}"
        ))
    })?;
    let content = document
        .pointer("/choices/0/message/content")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            AppError::ProviderExecution(
                "the text service returned no suggestion content".into(),
            )
        })?;
    let mut suggestion: serde_json::Value = serde_json::from_str(content)
        .or_else::<AppError, _>(|_| {
            // Some compatible services wrap JSON in a fenced code block.
            let stripped = content
                .trim()
                .trim_start_matches("```json")
                .trim_start_matches("```")
                .trim_end_matches("```");
            serde_json::from_str(stripped).map_err(|_| {
                AppError::ProviderExecution(
                    "the text service suggestion was not valid JSON".into(),
                )
            })
        })?;
    if !suggestion.is_object() {
        return Err(AppError::ProviderExecution(
            "the text service suggestion was not an object".into(),
        ));
    }
    // Keep only the known fields so a chatty model cannot inject junk into
    // the form.
    if let Some(map) = suggestion.as_object_mut() {
        map.retain(|key, _| VISUAL_SPEC_FIELDS.contains(&key.as_str()) || key == "baselineWardrobe");
        map.values_mut().for_each(|value| {
            if !value.is_string() {
                *value = serde_json::Value::String(value.to_string());
            }
        });
    }
    Ok(suggestion)
}

fn first_llm_provider(
    project_root: &Path,
) -> Result<Option<super::model::CustomProviderDefinition>, AppError> {
    let conn = db::open_existing_connection(&project_root.join("project.db"))?;
    let definitions = super::repository::list_custom_providers(&conn)?;
    Ok(definitions
        .into_iter()
        .find(|definition| definition.purpose == super::model::CustomProviderPurpose::Llm))
}

const SUGGESTION_SYSTEM_PROMPT: &str = r#"You help fill in a photoreal character "visual spec" for a cinematic AI production tool. Given a character name and optional context, propose concise, concrete descriptors for each field.

Rules:
- Describe visible, measurable things: geometry, muscle, tone, texture. Never mood words ("angry" is wrong; "brows drawn together and down at the inner ends, jaw set" is right).
- Skin: tone with a specifier (e.g. "warm fair", "olive medium", "cool deep") plus finish.
- Hair: color with nuance, length, texture, style.
- Each field: one short phrase, 4 to 16 words. English only.
- When a trait risks drifting, end the phrase with a short "never" clause (e.g. "cool ash platinum, never brassy").
- Baseline wardrobe: one garment-level outfit sentence, plain and specific, no brands.
- Do not invent identity markers the user did not mention (scars, tattoos) unless the context explicitly includes them.

Respond with a JSON object with exactly these string keys:
head, eyes, brows, nose, lips, skin, hair, build, expression, baselineWardrobe"#;
