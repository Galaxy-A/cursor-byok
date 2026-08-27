use std::{collections::HashSet, path::Path};

use serde::Deserialize;

use crate::{
    model::{
        model_hash, normalize_model_input, normalize_request_url, ModelConfigInput, ModelType,
        OPENAI_CHAT_ENDPOINT, OPENAI_RESPONSES_ENDPOINT,
    },
    Error, Result,
};

use super::Store;

pub struct LegacyModelImportPlan {
    pub models: Vec<LegacyModelImportEntry>,
}

pub struct LegacyModelImportEntry {
    pub model_hash: String,
    pub input: ModelConfigInput,
    pub existing: bool,
}

pub struct LegacyModelImportOutcome {
    pub imported: usize,
    pub skipped: usize,
    pub total: usize,
}

#[derive(Default, Deserialize)]
struct LegacyConfig {
    #[serde(rename = "modelAdapters", default)]
    model_adapters: Vec<LegacyModel>,
}

#[derive(Default, Deserialize)]
struct LegacyModel {
    #[serde(default)]
    sort: i64,
    #[serde(rename = "displayName", default)]
    display_name: String,
    #[serde(rename = "type", default)]
    model_type: String,
    #[serde(rename = "baseURL", default)]
    base_url: String,
    #[serde(rename = "apiKey", default)]
    api_key: String,
    #[serde(rename = "tooltipData", default)]
    tooltip_data: String,
    #[serde(rename = "modelID", default)]
    model_id: String,
    #[serde(rename = "reasoningEffort", default)]
    reasoning_effort: String,
    #[serde(rename = "openAIEndpoint", default)]
    openai_endpoint: String,
    #[serde(rename = "codexOutboundEnabled", default)]
    codex_outbound_enabled: bool,
    #[serde(rename = "openAIExtraParamsEnabled", default)]
    openai_extra_params_enabled: bool,
    #[serde(rename = "openAIExtraParamsJSON", default)]
    openai_extra_params_json: String,
    #[serde(rename = "customHeadersEnabled", default)]
    custom_headers_enabled: bool,
    #[serde(rename = "customHeadersJSON", default)]
    custom_headers_json: String,
    #[serde(rename = "anthropicExtraParamsEnabled", default)]
    anthropic_extra_params_enabled: bool,
    #[serde(rename = "anthropicExtraParamsJSON", default)]
    anthropic_extra_params_json: String,
    #[serde(rename = "contextWindowTokens", default)]
    context_window_tokens: u64,
    #[serde(rename = "maxCompletionTokens", default)]
    max_completion_tokens: u64,
    #[serde(rename = "anthropicMaxTokens", default)]
    anthropic_max_tokens: u64,
    #[serde(rename = "anthropicThinkingEffort", default)]
    anthropic_thinking_effort: String,
    #[serde(rename = "thinkingBudgetTokens", default)]
    thinking_budget_tokens: u64,
}

impl Store {
    pub async fn preview_v0049_model_config(&self, path: &Path) -> Result<LegacyModelImportPlan> {
        let inputs = load_v0049_model_config(path)?;
        let existing = self
            .models()
            .await?
            .into_iter()
            .map(|model| model.model_hash)
            .collect::<HashSet<_>>();
        let mut seen = HashSet::with_capacity(inputs.len());
        let mut models = Vec::with_capacity(inputs.len());
        for input in inputs {
            let input = normalize_model_input(&input)?;
            let hash = model_hash(&input)?;
            if seen.insert(hash.clone()) {
                models.push(LegacyModelImportEntry {
                    existing: existing.contains(&hash),
                    model_hash: hash,
                    input,
                });
            }
        }
        Ok(LegacyModelImportPlan { models })
    }

    pub async fn import_v0049_model_config(&self, path: &Path) -> Result<LegacyModelImportOutcome> {
        let plan = self.preview_v0049_model_config(path).await?;
        let total = plan.models.len();
        let missing = plan
            .models
            .into_iter()
            .filter(|model| !model.existing)
            .map(|model| model.input)
            .collect::<Vec<_>>();
        let imported = self.create_models_if_missing(&missing).await?;
        Ok(LegacyModelImportOutcome {
            imported,
            skipped: total - imported,
            total,
        })
    }
}

fn load_v0049_model_config(path: &Path) -> Result<Vec<ModelConfigInput>> {
    let raw = match std::fs::read(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::Config(format!(
                "v0.0.49 config not found at {}",
                path.display()
            )))
        }
        Err(error) => return Err(error.into()),
    };
    let legacy: LegacyConfig = serde_yaml::from_slice(&raw)
        .map_err(|error| Error::Config(format!("invalid v0.0.49 config: {error}")))?;
    if legacy.model_adapters.is_empty() {
        return Err(Error::Config(
            "v0.0.49 config contains no model adapters".into(),
        ));
    }
    let models = legacy
        .model_adapters
        .into_iter()
        .map(model_input)
        .collect::<Result<Vec<_>>>()?;
    Ok(models)
}

fn model_input(model: LegacyModel) -> Result<ModelConfigInput> {
    let model_type = match model.model_type.trim().to_ascii_lowercase().as_str() {
        "openai" => ModelType::OpenAi,
        "anthropic" => ModelType::Anthropic,
        value => {
            return Err(Error::Config(format!(
                "unsupported v0.0.49 model type: {value}"
            )))
        }
    };
    let (base_url, openai_endpoint, use_full_url) =
        legacy_request_configuration(model_type, &model.base_url, &model.openai_endpoint)?;
    let codex_outbound_enabled = model_type == ModelType::OpenAi && model.codex_outbound_enabled;
    Ok(ModelConfigInput {
        sort_order: model.sort,
        display_name: model.display_name.clone(),
        model_type,
        base_url,
        use_full_url,
        api_key: model.api_key,
        tooltip_data: if model.tooltip_data.trim().is_empty() {
            model.display_name
        } else {
            model.tooltip_data
        },
        model_id: model.model_id,
        reasoning_effort: optional_string(model.reasoning_effort),
        openai_endpoint: if codex_outbound_enabled {
            OPENAI_RESPONSES_ENDPOINT.into()
        } else {
            openai_endpoint
        },
        codex_outbound_enabled,
        openai_extra_params_enabled: model.openai_extra_params_enabled,
        openai_extra_params: enabled_json_object(
            model_type == ModelType::OpenAi && model.openai_extra_params_enabled,
            &model.openai_extra_params_json,
        )?,
        custom_headers_enabled: model.custom_headers_enabled,
        custom_headers: enabled_json_object(
            model.custom_headers_enabled,
            &model.custom_headers_json,
        )?,
        anthropic_extra_params_enabled: model.anthropic_extra_params_enabled,
        anthropic_extra_params: enabled_json_object(
            model_type == ModelType::Anthropic && model.anthropic_extra_params_enabled,
            &model.anthropic_extra_params_json,
        )?,
        context_window_tokens: positive(model.context_window_tokens),
        max_completion_tokens: positive(model.max_completion_tokens),
        anthropic_max_tokens: positive(model.anthropic_max_tokens),
        anthropic_thinking_effort: optional_string(model.anthropic_thinking_effort),
        thinking_budget_tokens: positive(model.thinking_budget_tokens),
    })
}

fn legacy_request_configuration(
    model_type: ModelType,
    base_url: &str,
    openai_endpoint: &str,
) -> Result<(String, String, bool)> {
    let base_url = normalize_request_url(base_url)?;
    match model_type {
        ModelType::Anthropic => {
            let use_full_url = url_path_ends_with(&base_url, "/messages");
            Ok((base_url, String::new(), use_full_url))
        }
        ModelType::OpenAi => {
            let detected = openai_protocol_from_url(&base_url);
            let configured = match openai_endpoint.trim() {
                "" | OPENAI_RESPONSES_ENDPOINT => OPENAI_RESPONSES_ENDPOINT,
                OPENAI_CHAT_ENDPOINT => OPENAI_CHAT_ENDPOINT,
                "/custom" => OPENAI_CHAT_ENDPOINT,
                value => {
                    return Err(Error::Config(format!(
                        "unsupported v0.0.49 OpenAI endpoint: {value}"
                    )))
                }
            };
            let protocol = detected.unwrap_or(configured);
            let use_full_url = detected.is_some() || openai_endpoint.trim() == "/custom";
            Ok((base_url, protocol.into(), use_full_url))
        }
    }
}

fn openai_protocol_from_url(value: &str) -> Option<&'static str> {
    let url = reqwest::Url::parse(value).ok()?;
    let path = url.path().trim_end_matches('/');
    if path.to_ascii_lowercase().ends_with("/responses") {
        Some(OPENAI_RESPONSES_ENDPOINT)
    } else if path.to_ascii_lowercase().ends_with("/chat/completions") {
        Some(OPENAI_CHAT_ENDPOINT)
    } else {
        None
    }
}

fn url_path_ends_with(value: &str, suffix: &str) -> bool {
    reqwest::Url::parse(value).is_ok_and(|url| {
        url.path()
            .trim_end_matches('/')
            .to_ascii_lowercase()
            .ends_with(suffix)
    })
}

fn enabled_json_object(enabled: bool, value: &str) -> Result<serde_json::Value> {
    if enabled {
        json_object(value)
    } else {
        Ok(serde_json::json!({}))
    }
}

fn json_object(value: &str) -> Result<serde_json::Value> {
    if value.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    let value: serde_json::Value = serde_json::from_str(value)?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(Error::Config(
            "v0.0.49 model JSON fields must be objects".into(),
        ))
    }
}

fn positive(value: u64) -> Option<u64> {
    (value > 0).then_some(value)
}

fn optional_string(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn manually_imports_v0049_models_as_complete_request_urls() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("config.yaml");
        std::fs::write(
            &config,
            r#"modelAdapters:
  - sort: 1
    displayName: Model A
    type: openai
    baseURL: https://example.com/v1
    apiKey: secret
    tooltipData: Example model
    modelID: model-a
    reasoningEffort: high
    openAIEndpoint: /v1/responses
    openAIExtraParamsEnabled: true
    openAIExtraParamsJSON: '{"service_tier":"priority"}'
    customHeadersEnabled: true
    customHeadersJSON: '{"x-client":"cursor-byok"}'
    contextWindowTokens: 200000
    maxCompletionTokens: 8192
  - sort: 2
    displayName: Custom Chat
    type: openai
    baseURL: https://example.com/proxy/generate?api-version=2026-01-01
    apiKey: secret
    modelID: model-b
    openAIEndpoint: /custom
    openAIExtraParamsEnabled: false
    openAIExtraParamsJSON: not-valid-json
  - sort: 3
    displayName: Claude
    type: anthropic
    baseURL: https://example.com/anthropic
    apiKey: secret
    modelID: model-c
    customHeadersEnabled: false
    customHeadersJSON: not-valid-json
"#,
        )
        .unwrap();
        let store = Store::connect("sqlite::memory:").await.unwrap();

        let preview = store.preview_v0049_model_config(&config).await.unwrap();
        assert_eq!(preview.models.len(), 3);
        assert!(preview.models.iter().all(|model| !model.existing));
        store.create_model(&preview.models[0].input).await.unwrap();
        let preview = store.preview_v0049_model_config(&config).await.unwrap();
        assert_eq!(
            preview.models.iter().filter(|model| model.existing).count(),
            1
        );
        let first = store.import_v0049_model_config(&config).await.unwrap();
        assert_eq!(first.imported, 2);
        assert_eq!(first.skipped, 1);
        assert_eq!(first.total, 3);
        let models = store.models().await.unwrap();
        assert_eq!(models.len(), 3);
        assert_eq!(models[0].model_hash.len(), 16);
        assert_eq!(models[0].base_url, "https://example.com/v1");
        assert!(!models[0].use_full_url);
        assert_eq!(
            models[0].request_url().unwrap(),
            "https://example.com/v1/responses"
        );
        assert_eq!(models[0].openai_extra_params["service_tier"], "priority");
        assert_eq!(
            models[1].base_url,
            "https://example.com/proxy/generate?api-version=2026-01-01"
        );
        assert_eq!(models[1].openai_endpoint, OPENAI_CHAT_ENDPOINT);
        assert!(models[1].use_full_url);
        assert_eq!(models[1].openai_extra_params, serde_json::json!({}));
        assert_eq!(
            models[2].request_url().unwrap(),
            "https://example.com/anthropic/v1/messages"
        );
        assert!(!models[2].use_full_url);
        assert_eq!(models[2].custom_headers, serde_json::json!({}));
        let preview = store.preview_v0049_model_config(&config).await.unwrap();
        assert!(preview.models.iter().all(|model| model.existing));
        let second = store.import_v0049_model_config(&config).await.unwrap();
        assert_eq!(second.imported, 0);
        assert_eq!(second.skipped, 3);
        assert_eq!(second.total, 3);
        assert_eq!(store.models().await.unwrap().len(), 3);
    }

    #[test]
    fn v0049_anthropic_full_request_url_is_not_modified() {
        let (request_url, endpoint, use_full_url) = legacy_request_configuration(
            ModelType::Anthropic,
            "https://example.com/proxy/messages?api-version=2026-01-01",
            "",
        )
        .unwrap();

        assert_eq!(
            request_url,
            "https://example.com/proxy/messages?api-version=2026-01-01"
        );
        assert!(endpoint.is_empty());
        assert!(use_full_url);
    }
}
