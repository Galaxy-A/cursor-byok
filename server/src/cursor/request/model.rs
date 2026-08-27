use crate::{
    cursor::proto::agent::v1 as pb,
    model::{
        parse_token_count, ModelLatency, ModelSpec, ReasoningSpec, SubagentKind,
        SubagentModelOverride,
    },
    Error, Result,
};

pub fn requested_model(request: &pb::AgentRunRequest) -> Result<ModelSpec> {
    let details = request.model_details.as_ref();
    let model = if let Some(requested) = request.requested_model.as_ref() {
        from_requested(requested, details)?
    } else if let Some(model_id) = details
        .map(|model| model.model_id.as_str())
        .filter(|model| !model.is_empty())
    {
        ModelSpec {
            model_id: model_id.into(),
            display_name: details
                .map(|model| model.display_name.clone())
                .filter(|name| !name.is_empty()),
            reasoning: ReasoningSpec {
                enabled: details.is_some_and(|model| model.thinking_details.is_some()),
                effort: None,
            },
            latency: ModelLatency::Standard,
            max_output_tokens: None,
            context_window_tokens: None,
            supports_image_generation: false,
            codex_outbound_enabled: false,
            extra_params: serde_json::json!({}),
        }
    } else {
        return Err(Error::Protocol("Cursor Run does not select a model".into()));
    };
    Ok(model)
}

pub fn overrides(
    request: &pb::AgentRunRequest,
) -> Result<Vec<(SubagentKind, SubagentModelOverride)>> {
    request
        .subagent_model_overrides
        .iter()
        .map(|value| {
            use pb::subagent_model_override::Selection;
            let kind = subagent_kind(&value.subagent_type);
            let selection = match value.selection.as_ref() {
                Some(Selection::Model(model)) => {
                    if model.model_id == "default" {
                        SubagentModelOverride::Inherit
                    } else {
                        SubagentModelOverride::Explicit(from_requested(model, None)?)
                    }
                }
                Some(Selection::Inherit(true)) => SubagentModelOverride::Inherit,
                Some(Selection::Disabled(true)) => SubagentModelOverride::Disabled,
                None | Some(Selection::Inherit(false) | Selection::Disabled(false)) => {
                    return Err(Error::Protocol(format!(
                        "Cursor subagent model override {} has no active selection",
                        value.subagent_type
                    )))
                }
            };
            Ok((kind, selection))
        })
        .collect()
}

pub fn subagent_kind(value: &str) -> SubagentKind {
    if value == "generalPurpose" {
        SubagentKind::GeneralPurpose
    } else {
        SubagentKind::Named(value.into())
    }
}

fn from_requested(
    model: &pb::RequestedModel,
    details: Option<&pb::ModelDetails>,
) -> Result<ModelSpec> {
    let mut spec = ModelSpec {
        model_id: model.model_id.clone(),
        display_name: details
            .map(|model| model.display_name.clone())
            .filter(|name| !name.is_empty()),
        reasoning: ReasoningSpec {
            enabled: model.max_mode
                || details.is_some_and(|model| model.thinking_details.is_some()),
            effort: None,
        },
        latency: ModelLatency::Standard,
        max_output_tokens: None,
        context_window_tokens: None,
        supports_image_generation: false,
        codex_outbound_enabled: false,
        extra_params: serde_json::json!({}),
    };
    for parameter in &model.parameters {
        match parameter.id.as_str() {
            "effort" | "reasoning" => {
                let effort = parameter.value.trim();
                spec.reasoning.effort =
                    (effort != "none" && !effort.is_empty()).then(|| effort.to_string());
                spec.reasoning.enabled |= spec.reasoning.effort.is_some();
            }
            "thinking" => spec.reasoning.enabled |= parse_bool(parameter)?,
            "fast" => {
                if parse_bool(parameter)? {
                    spec.latency = ModelLatency::Fast;
                }
            }
            "context" => {
                spec.context_window_tokens =
                    Some(parse_token_count(&parameter.value).ok_or_else(|| {
                        Error::Protocol(format!(
                            "invalid Cursor context token count: {}",
                            parameter.value
                        ))
                    })?);
            }
            other => {
                return Err(Error::Protocol(format!(
                    "unsupported Cursor model parameter: {other}"
                )))
            }
        }
    }
    Ok(spec)
}

fn parse_bool(parameter: &pb::requested_model::ModelParameterValue) -> Result<bool> {
    match parameter.value.as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(Error::Protocol(format!(
            "invalid Cursor boolean model parameter {}={}",
            parameter.id, parameter.value
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requested(id: &str, parameters: &[(&str, &str)]) -> pb::RequestedModel {
        pb::RequestedModel {
            model_id: id.into(),
            parameters: parameters
                .iter()
                .map(|(id, value)| pb::requested_model::ModelParameterValue {
                    id: (*id).into(),
                    value: (*value).into(),
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn cursor_model_parameters_keep_order_and_define_reasoning() {
        let model = from_requested(
            &requested("grok-4.6", &[("effort", "xhigh"), ("fast", "false")]),
            None,
        )
        .unwrap();
        assert_eq!(model.model_id, "grok-4.6");
        assert!(model.reasoning.enabled);
        assert_eq!(model.reasoning.effort.as_deref(), Some("xhigh"));
        assert_eq!(model.latency, ModelLatency::Standard);
    }

    #[test]
    fn cursor_reasoning_and_context_metadata_are_normalized() {
        let model = from_requested(
            &requested(
                "gpt-5.6-sol",
                &[("context", "272k"), ("reasoning", "medium")],
            ),
            None,
        )
        .unwrap();
        assert_eq!(model.context_window_tokens, Some(272_000));
        assert_eq!(model.reasoning.effort.as_deref(), Some("medium"));
        assert!(model.reasoning.enabled);
    }

    #[test]
    fn cursor_catalog_context_values_are_consumed() {
        for (value, tokens) in [
            ("200k", 200_000),
            ("356k", 356_000),
            ("800k", 800_000),
            ("1m", 1_000_000),
        ] {
            let model = from_requested(&requested("model", &[("context", value)]), None).unwrap();
            assert_eq!(model.context_window_tokens, Some(tokens));
        }
    }

    #[test]
    fn subagent_override_distinguishes_explicit_inherit_and_disabled() {
        let request = pb::AgentRunRequest {
            subagent_model_overrides: vec![
                pb::SubagentModelOverride {
                    subagent_type: "explore".into(),
                    selection: Some(pb::subagent_model_override::Selection::Model(requested(
                        "claude-opus-5",
                        &[("thinking", "true")],
                    ))),
                },
                pb::SubagentModelOverride {
                    subagent_type: "generalPurpose".into(),
                    selection: Some(pb::subagent_model_override::Selection::Inherit(true)),
                },
                pb::SubagentModelOverride {
                    subagent_type: "shell".into(),
                    selection: Some(pb::subagent_model_override::Selection::Disabled(true)),
                },
            ],
            ..Default::default()
        };
        let overrides = overrides(&request).unwrap();
        assert!(matches!(
            &overrides[0],
            (SubagentKind::Named(name), SubagentModelOverride::Explicit(model))
                if name == "explore" && model.reasoning.enabled
        ));
        assert!(matches!(
            &overrides[1],
            (SubagentKind::GeneralPurpose, SubagentModelOverride::Inherit)
        ));
        assert!(matches!(
            &overrides[2],
            (SubagentKind::Named(name), SubagentModelOverride::Disabled) if name == "shell"
        ));
    }

    #[test]
    fn default_subagent_model_is_inherit() {
        let request = pb::AgentRunRequest {
            subagent_model_overrides: vec![pb::SubagentModelOverride {
                subagent_type: "generalPurpose".into(),
                selection: Some(pb::subagent_model_override::Selection::Model(requested(
                    "default",
                    &[],
                ))),
            }],
            ..Default::default()
        };

        assert!(matches!(
            overrides(&request).unwrap().as_slice(),
            [(SubagentKind::GeneralPurpose, SubagentModelOverride::Inherit)]
        ));
    }

    #[test]
    fn cursor_only_parameters_do_not_leak_into_model_spec() {
        let model = from_requested(
            &requested("grok-4.6", &[("fast", "true"), ("context", "300k")]),
            None,
        )
        .unwrap();
        assert_eq!(model.latency, ModelLatency::Fast);
        assert_eq!(model.context_window_tokens, Some(300_000));
        assert!(from_requested(&requested("grok-4.6", &[("mystery", "x")]), None).is_err());
    }
}
