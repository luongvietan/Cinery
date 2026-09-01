//! Provider presets: declarative, data-only descriptions of well-known
//! services. Presets compile into `ProviderRuntimeConfig`; there is no
//! provider-specific code anywhere else in the runtime.

use super::config::{
    AsyncJobConfig, AuthConfig, AuthMode, EndpointConfig, ErrorMapping, FinalOutputConfig,
    MultipartFieldConfig, MultipartFieldKind, PollingConfig, ProviderRuntimeConfig, RequestType,
    ResponseMapping, StatusEndpointConfig, OPERATION_IMAGE_EDIT, OPERATION_IMAGE_GENERATE,
    OPERATION_VALIDATE, OPERATION_VIDEO_GENERATE,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A preset for the SIMPLE configuration mode. Everything except the
/// user's credentials and account values is pre-configured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPreset {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    /// Internal presets back legacy migrations and are not offered in the UI.
    #[serde(default)]
    pub internal: bool,
    pub default_base_url: &'static str,
    pub requires_account_id: bool,
    pub auth: AuthConfig,
    /// Model ids offered by default; users can edit the list.
    pub default_models: Vec<(&'static str, &'static str)>,
    pub runtime: ProviderRuntimeConfig,
}

fn bearer_auth() -> AuthConfig {
    AuthConfig {
        mode: AuthMode::Bearer,
        credential_name: None,
    }
}

fn openai_image_generate_endpoint() -> EndpointConfig {
    EndpointConfig {
        method: "POST".into(),
        path_template: "/images/generations".into(),
        request_type: RequestType::Json,
        request_mapping: Some(serde_json::json!({
            "model": "{{model}}",
            "prompt": "{{prompt}}",
            "size": "1024x1024"
        })),
        response: ResponseMapping {
            outputs_path: Some("data".into()),
            url_path: Some("url".into()),
            base64_path: Some("b64_json".into()),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn openai_image_edit_endpoint() -> EndpointConfig {
    EndpointConfig {
        method: "POST".into(),
        path_template: "/images/edits".into(),
        request_type: RequestType::Multipart,
        multipart_fields: vec![
            MultipartFieldConfig {
                name: "model".into(),
                kind: MultipartFieldKind::Text,
                value: Some("{{model}}".into()),
                source: None,
            },
            MultipartFieldConfig {
                name: "prompt".into(),
                kind: MultipartFieldKind::Text,
                value: Some("{{prompt}}".into()),
                source: None,
            },
            MultipartFieldConfig {
                name: "input_fidelity".into(),
                kind: MultipartFieldKind::Text,
                value: Some("high".into()),
                source: None,
            },
            MultipartFieldConfig {
                name: "image[]".into(),
                kind: MultipartFieldKind::File,
                value: None,
                source: Some("images".into()),
            },
        ],
        response: ResponseMapping {
            outputs_path: Some("data".into()),
            url_path: Some("url".into()),
            base64_path: Some("b64_json".into()),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn openai_validate_endpoint() -> EndpointConfig {
    EndpointConfig {
        method: "GET".into(),
        path_template: "/models".into(),
        response: ResponseMapping {
            binary_response: true,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// OpenAI-compatible video job API (`/videos`): submit → poll → download.
/// Used for legacy video-purpose providers.
pub fn openai_compatible_video_runtime() -> ProviderRuntimeConfig {
    let mut operations = BTreeMap::new();
    operations.insert(
        OPERATION_VIDEO_GENERATE.to_string(),
        EndpointConfig {
            method: "POST".into(),
            path_template: "/videos".into(),
            request_type: RequestType::Json,
            request_mapping: Some(serde_json::json!({
                "model": "{{model}}",
                "prompt": "{{prompt}}"
            })),
            response: ResponseMapping::default(),
            job: Some(AsyncJobConfig {
                job_id_path: "id".into(),
                status: StatusEndpointConfig {
                    method: "GET".into(),
                    path_template: "/videos/{jobId}".into(),
                    status_path: "status".into(),
                    completed_values: vec!["completed".into()],
                    failed_values: vec!["failed".into(), "cancelled".into()],
                    progress_path: Some("progress".into()),
                    error_message_path: Some("error".into()),
                },
                output: FinalOutputConfig {
                    fetch_path_template: None,
                    fetch_method: "GET".into(),
                    response: ResponseMapping {
                        url_path: Some("url".into()),
                        ..Default::default()
                    },
                },
                polling: PollingConfig {
                    interval_ms: 3000,
                    timeout_ms: 600_000,
                },
            }),
            ..Default::default()
        },
    );
    ProviderRuntimeConfig {
        auth: bearer_auth(),
        operations,
        error_mapping: Some(ErrorMapping {
            message_path: Some("error.message".into()),
            code_path: None,
            request_id_path: None,
        }),
        ..Default::default()
    }
}

fn cloudflare_image_generate_endpoint() -> EndpointConfig {
    EndpointConfig {
        method: "POST".into(),
        path_template: "/{model}".into(),
        request_type: RequestType::Json,
        request_mapping: Some(serde_json::json!({
            "prompt": "{{prompt}}",
            "steps": "{{steps}}"
        })),
        response: ResponseMapping {
            base64_path: Some("result.image".into()),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn cloudflare_validate_endpoint() -> EndpointConfig {
    EndpointConfig {
        method: "POST".into(),
        path_template: "/{model}".into(),
        request_type: RequestType::Json,
        request_mapping: Some(serde_json::json!({
            "prompt": "simple provider validation test",
            "steps": 1
        })),
        response: ResponseMapping {
            binary_response: true,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// The full preset catalog. Internal presets are used for legacy migrations.
pub fn all_presets() -> Vec<ProviderPreset> {
    vec![
        // --- OpenAI-compatible -------------------------------------------
        ProviderPreset {
            id: "openai-compatible",
            label: "OpenAI Compatible",
            description: "Any service speaking the OpenAI images API (/v1/images/generations). \
                          Works with OpenAI and OpenAI-compatible gateways.",
            internal: false,
            default_base_url: "https://api.openai.com/v1",
            requires_account_id: false,
            auth: bearer_auth(),
            default_models: vec![("gpt-image-2", "GPT Image 2")],
            runtime: {
                let mut operations = BTreeMap::new();
                operations.insert(
                    OPERATION_IMAGE_GENERATE.to_string(),
                    openai_image_generate_endpoint(),
                );
                operations.insert(
                    OPERATION_IMAGE_EDIT.to_string(),
                    openai_image_edit_endpoint(),
                );
                operations.insert(OPERATION_VALIDATE.to_string(), openai_validate_endpoint());
                ProviderRuntimeConfig {
                    auth: bearer_auth(),
                    operations,
                    ..Default::default()
                }
            },
        },
        // --- Cloudflare Workers AI ---------------------------------------
        ProviderPreset {
            id: "cloudflare-workers-ai",
            label: "Cloudflare Workers AI",
            description: "Run image models like FLUX.1 Schnell on Cloudflare's edge network. \
                          Needs your Cloudflare account ID and an API token.",
            internal: false,
            default_base_url: "https://api.cloudflare.com/client/v4/accounts/{accountId}/ai/run",
            requires_account_id: true,
            auth: bearer_auth(),
            default_models: vec![("@cf/black-forest-labs/flux-1-schnell", "FLUX.1 Schnell")],
            runtime: {
                let mut operations = BTreeMap::new();
                operations.insert(
                    OPERATION_IMAGE_GENERATE.to_string(),
                    cloudflare_image_generate_endpoint(),
                );
                operations.insert(
                    OPERATION_VALIDATE.to_string(),
                    cloudflare_validate_endpoint(),
                );
                ProviderRuntimeConfig {
                    auth: bearer_auth(),
                    operations,
                    error_mapping: Some(ErrorMapping {
                        message_path: Some("errors.0.message".into()),
                        code_path: Some("errors.0.code".into()),
                        request_id_path: None,
                    }),
                    ..Default::default()
                }
            },
        },
        // --- Pollinations --------------------------------------------------
        ProviderPreset {
            id: "pollinations",
            label: "Pollinations",
            description: "Free, keyless image generation. No API key required.",
            internal: false,
            default_base_url: "https://image.pollinations.ai",
            requires_account_id: false,
            auth: AuthConfig::default(),
            default_models: vec![("flux", "FLUX")],
            runtime: {
                let mut operations = BTreeMap::new();
                operations.insert(
                    OPERATION_IMAGE_GENERATE.to_string(),
                    EndpointConfig {
                        method: "GET".into(),
                        path_template: "/prompt/{{prompt}}?width={{width}}&height={{height}}&seed={{seed}}&nologo=true"
                            .into(),
                        request_type: RequestType::Json,
                        request_mapping: Some(serde_json::json!({})),
                        response: ResponseMapping {
                            binary_response: true,
                            mime_type: "image/jpeg".into(),
                            filename: "generated.jpg".into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                );
                operations.insert(
                    OPERATION_VALIDATE.to_string(),
                    EndpointConfig {
                        method: "GET".into(),
                        path_template: "/models".into(),
                        response: ResponseMapping {
                            binary_response: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                );
                ProviderRuntimeConfig {
                    auth: AuthConfig::default(),
                    operations,
                    ..Default::default()
                }
            },
        },
        // --- Runware -------------------------------------------------------
        ProviderPreset {
            id: "runware",
            label: "Runware",
            description: "Fast, inexpensive image generation via the Runware task API.",
            internal: false,
            default_base_url: "https://api.runware.ai/v1",
            requires_account_id: false,
            auth: bearer_auth(),
            default_models: vec![("runware:100@1", "FLUX Schnell")],
            runtime: {
                let mut operations = BTreeMap::new();
                operations.insert(
                    OPERATION_IMAGE_GENERATE.to_string(),
                    EndpointConfig {
                        method: "POST".into(),
                        path_template: String::new(),
                        request_type: RequestType::Json,
                        request_mapping: Some(serde_json::json!([{
                            "taskType": "imageInference",
                            "taskUUID": "cinery-{{seed}}",
                            "positivePrompt": "{{prompt}}",
                            "model": "{{model}}",
                            "width": 1024,
                            "height": 1024
                        }])),
                        response: ResponseMapping {
                            url_path: Some("data.0.imageURL".into()),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                );
                ProviderRuntimeConfig {
                    auth: bearer_auth(),
                    operations,
                    ..Default::default()
                }
            },
        },
        // --- Replicate -----------------------------------------------------
        ProviderPreset {
            id: "replicate",
            label: "Replicate",
            description: "Run community and official models on Replicate. \
                          Model IDs look like owner/name.",
            internal: false,
            default_base_url: "https://api.replicate.com/v1",
            requires_account_id: false,
            auth: bearer_auth(),
            default_models: vec![("black-forest-labs/flux-schnell", "FLUX Schnell")],
            runtime: {
                let mut operations = BTreeMap::new();
                operations.insert(
                    OPERATION_IMAGE_GENERATE.to_string(),
                    EndpointConfig {
                        method: "POST".into(),
                        path_template: "/models/{model}/predictions".into(),
                        request_type: RequestType::Json,
                        request_mapping: Some(serde_json::json!({
                            "input": {"prompt": "{{prompt}}"}
                        })),
                        response: ResponseMapping::default(),
                        job: Some(AsyncJobConfig {
                            job_id_path: "id".into(),
                            status: StatusEndpointConfig {
                                method: "GET".into(),
                                path_template: "/predictions/{jobId}".into(),
                                status_path: "status".into(),
                                completed_values: vec!["succeeded".into()],
                                failed_values: vec!["failed".into(), "canceled".into()],
                                progress_path: None,
                                error_message_path: Some("error".into()),
                            },
                            output: FinalOutputConfig {
                                fetch_path_template: None,
                                fetch_method: "GET".into(),
                                response: ResponseMapping {
                                    url_path: Some("output.0".into()),
                                    ..Default::default()
                                },
                            },
                            polling: PollingConfig {
                                interval_ms: 2000,
                                timeout_ms: 600_000,
                            },
                        }),
                        ..Default::default()
                    },
                );
                operations.insert(
                    OPERATION_VALIDATE.to_string(),
                    EndpointConfig {
                        method: "GET".into(),
                        path_template: "/models/{model}".into(),
                        response: ResponseMapping {
                            binary_response: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                );
                ProviderRuntimeConfig {
                    auth: bearer_auth(),
                    operations,
                    error_mapping: Some(ErrorMapping {
                        message_path: Some("detail".into()),
                        code_path: None,
                        request_id_path: None,
                    }),
                    ..Default::default()
                }
            },
        },
        // --- fal.ai --------------------------------------------------------
        ProviderPreset {
            id: "fal",
            label: "fal.ai",
            description: "Synchronous generation endpoints for fal.ai models. \
                          Model IDs look like fal-ai/flux/schnell.",
            internal: false,
            default_base_url: "https://fal.run",
            requires_account_id: false,
            auth: bearer_auth(),
            default_models: vec![("fal-ai/flux/schnell", "FLUX Schnell (fal)")],
            runtime: {
                let mut operations = BTreeMap::new();
                operations.insert(
                    OPERATION_IMAGE_GENERATE.to_string(),
                    EndpointConfig {
                        method: "POST".into(),
                        path_template: "/{model}".into(),
                        request_type: RequestType::Json,
                        request_mapping: Some(serde_json::json!({
                            "prompt": "{{prompt}}"
                        })),
                        response: ResponseMapping {
                            outputs_path: Some("images".into()),
                            url_path: Some("url".into()),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                );
                ProviderRuntimeConfig {
                    auth: bearer_auth(),
                    operations,
                    ..Default::default()
                }
            },
        },
        // --- Alibaba / Wan (DashScope) -------------------------------------
        ProviderPreset {
            id: "alibaba-wan",
            label: "Alibaba Wan (DashScope)",
            description: "Video generation through Alibaba Model Studio / DashScope. \
                          Jobs are submitted asynchronously and polled.",
            internal: false,
            default_base_url: "https://dashscope-intl.aliyuncs.com/api/v1",
            requires_account_id: false,
            auth: bearer_auth(),
            default_models: vec![("wan2.2-t2v-plus", "Wan 2.2 Text-to-Video Plus")],
            runtime: {
                let mut operations = BTreeMap::new();
                let mut headers = BTreeMap::new();
                headers.insert("X-DashScope-Async".to_string(), "enable".to_string());
                operations.insert(
                    OPERATION_VIDEO_GENERATE.to_string(),
                    EndpointConfig {
                        method: "POST".into(),
                        path_template: "/services/aigc/video-generation/video-synthesis".into(),
                        request_type: RequestType::Json,
                        request_mapping: Some(serde_json::json!({
                            "model": "{{model}}",
                            "input": {"prompt": "{{prompt}}"}
                        })),
                        response: ResponseMapping::default(),
                        job: Some(AsyncJobConfig {
                            job_id_path: "output.task_id".into(),
                            status: StatusEndpointConfig {
                                method: "GET".into(),
                                path_template: "/tasks/{jobId}".into(),
                                status_path: "output.task_status".into(),
                                completed_values: vec!["SUCCEEDED".into()],
                                failed_values: vec![
                                    "FAILED".into(),
                                    "CANCELED".into(),
                                    "UNKNOWN".into(),
                                ],
                                progress_path: None,
                                error_message_path: Some("output.message".into()),
                            },
                            output: FinalOutputConfig {
                                fetch_path_template: None,
                                fetch_method: "GET".into(),
                                response: ResponseMapping {
                                    url_path: Some("output.video_url".into()),
                                    ..Default::default()
                                },
                            },
                            polling: PollingConfig {
                                interval_ms: 5000,
                                timeout_ms: 900_000,
                            },
                        }),
                        headers,
                        ..Default::default()
                    },
                );
                ProviderRuntimeConfig {
                    auth: bearer_auth(),
                    operations,
                    ..Default::default()
                }
            },
        },
        // --- Custom REST API -----------------------------------------------
        ProviderPreset {
            id: "custom",
            label: "Custom REST API",
            description: "Connect any HTTP endpoint: configure the request shape, \
                          authentication, and where to find the result in the response.",
            internal: false,
            default_base_url: "",
            requires_account_id: false,
            auth: bearer_auth(),
            default_models: vec![("default", "Default model")],
            runtime: {
                let mut operations = BTreeMap::new();
                operations.insert(
                    OPERATION_IMAGE_GENERATE.to_string(),
                    EndpointConfig {
                        method: "POST".into(),
                        path_template: "/generate".into(),
                        request_type: RequestType::Json,
                        request_mapping: Some(serde_json::json!({
                            "model": "{{model}}",
                            "prompt": "{{prompt}}"
                        })),
                        response: ResponseMapping {
                            url_path: Some("result.images.0.url".into()),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                );
                ProviderRuntimeConfig {
                    auth: bearer_auth(),
                    operations,
                    ..Default::default()
                }
            },
        },
        // --- Legacy migration targets (internal) ----------------------------
        ProviderPreset {
            id: "openai-compatible-video",
            label: "OpenAI Compatible Video",
            description: "OpenAI-compatible /videos job API (legacy migration target).",
            internal: true,
            default_base_url: "https://api.openai.com/v1",
            requires_account_id: false,
            auth: bearer_auth(),
            default_models: vec![],
            runtime: openai_compatible_video_runtime(),
        },
    ]
}

pub fn preset_by_id(id: &str) -> Option<ProviderPreset> {
    all_presets().into_iter().find(|preset| preset.id == id)
}

/// Runtime config synthesized for legacy purpose-based provider rows so old
/// records keep working without modification.
pub fn legacy_purpose_runtime(
    purpose: super::model::CustomProviderPurpose,
) -> ProviderRuntimeConfig {
    match purpose {
        super::model::CustomProviderPurpose::Image => {
            preset_by_id("openai-compatible")
                .expect("openai-compatible preset exists")
                .runtime
        }
        super::model::CustomProviderPurpose::Video => {
            preset_by_id("openai-compatible-video")
                .expect("openai-compatible-video preset exists")
                .runtime
        }
        // LLM and legacy providers do not use declarative media operations.
        _ => ProviderRuntimeConfig::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_public_preset_runtime_is_valid() {
        for preset in all_presets() {
            if preset.internal {
                continue;
            }
            preset
                .runtime
                .validate()
                .unwrap_or_else(|error| panic!("preset {} invalid: {error}", preset.id));
        }
    }

    #[test]
    fn cloudflare_preset_matches_the_acceptance_contract() {
        let preset = preset_by_id("cloudflare-workers-ai").unwrap();
        assert!(preset.requires_account_id);
        assert_eq!(
            preset.default_base_url,
            "https://api.cloudflare.com/client/v4/accounts/{accountId}/ai/run"
        );
        assert_eq!(
            preset.default_models[0].0,
            "@cf/black-forest-labs/flux-1-schnell"
        );
        let endpoint = preset
            .runtime
            .operations
            .get(OPERATION_IMAGE_GENERATE)
            .unwrap();
        assert_eq!(endpoint.path_template, "/{model}");
        assert_eq!(
            endpoint.request_mapping.as_ref().unwrap()["prompt"],
            "{{prompt}}"
        );
        assert_eq!(
            endpoint.response.base64_path.as_deref(),
            Some("result.image")
        );
        let validate = preset.runtime.operations.get(OPERATION_VALIDATE).unwrap();
        assert_eq!(validate.path_template, "/{model}");
        assert_eq!(validate.method, "POST");
    }

    #[test]
    fn openai_preset_covers_generation_edit_and_validation() {
        let preset = preset_by_id("openai-compatible").unwrap();
        assert!(preset
            .runtime
            .operations
            .contains_key(OPERATION_IMAGE_GENERATE));
        assert!(preset.runtime.operations.contains_key(OPERATION_IMAGE_EDIT));
        assert!(preset.runtime.operations.contains_key(OPERATION_VALIDATE));
    }

    #[test]
    fn async_presets_define_complete_job_lifecycles() {
        let replicate = preset_by_id("replicate").unwrap();
        let job = replicate
            .runtime
            .operations
            .get(OPERATION_IMAGE_GENERATE)
            .unwrap()
            .job
            .as_ref()
            .unwrap();
        assert_eq!(job.job_id_path, "id");
        assert_eq!(job.status.completed_values, vec!["succeeded"]);

        let wan = preset_by_id("alibaba-wan").unwrap();
        let job = wan
            .runtime
            .operations
            .get(OPERATION_VIDEO_GENERATE)
            .unwrap()
            .job
            .as_ref()
            .unwrap();
        assert_eq!(job.job_id_path, "output.task_id");
        assert_eq!(job.status.status_path, "output.task_status");
    }

    #[test]
    fn legacy_synthesis_matches_previous_purpose_behavior() {
        let image = legacy_purpose_runtime(super::super::model::CustomProviderPurpose::Image);
        assert!(image.operations.contains_key(OPERATION_IMAGE_GENERATE));
        assert!(image.operations.contains_key(OPERATION_IMAGE_EDIT));
        assert_eq!(image.auth.mode, AuthMode::Bearer);
        let video = legacy_purpose_runtime(super::super::model::CustomProviderPurpose::Video);
        assert!(video.operations.contains_key(OPERATION_VIDEO_GENERATE));
        assert!(video
            .operations
            .get(OPERATION_VIDEO_GENERATE)
            .unwrap()
            .job
            .is_some());
    }
}
