use cursor_server::{
    model::{
        ConversationId, ModelConfigInput, ModelSpec, ModelType, NewLlmCall, PreparedRun,
        PromptSpec, ProviderType, RunAction, RunId, RunKind, Usage, OPENAI_CHAT_ENDPOINT,
    },
    store::{RunStatus, Store},
};

async fn store() -> (tempfile::TempDir, Store) {
    let directory = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", directory.path().join("test.db").display());
    let store = Store::connect(&url).await.unwrap();
    (directory, store)
}

fn model_input() -> ModelConfigInput {
    ModelConfigInput {
        sort_order: 0,
        display_name: "Model A".into(),
        model_type: ModelType::OpenAi,
        base_url: "https://example.com/v1/chat/completions".into(),
        use_full_url: true,
        api_key: "secret".into(),
        tooltip_data: "Model A".into(),
        model_id: "model-a".into(),
        reasoning_effort: None,
        openai_endpoint: OPENAI_CHAT_ENDPOINT.into(),
        codex_outbound_enabled: false,
        openai_extra_params_enabled: true,
        openai_extra_params: serde_json::json!({"temperature":0}),
        custom_headers_enabled: true,
        custom_headers: serde_json::json!({"x-route":"one"}),
        anthropic_extra_params_enabled: false,
        anthropic_extra_params: serde_json::json!({}),
        context_window_tokens: None,
        max_completion_tokens: None,
        anthropic_max_tokens: None,
        anthropic_thinking_effort: None,
        thinking_budget_tokens: None,
    }
}

#[tokio::test]
async fn model_configuration_round_trips_and_hash_uses_v0049_identity() {
    let (_directory, store) = store().await;
    let model = store.create_model(&model_input()).await.unwrap();
    assert_eq!(model.model_hash.len(), 16);
    assert_eq!(model.api_key, "secret");
    assert_eq!(model.custom_headers["x-route"], "one");

    let original_hash = model.model_hash.clone();
    let mut input = model_input();
    input.base_url = "https://example.com/v1/chat/completions".into();
    input.sort_order = 3;
    input.tooltip_data = "Updated tooltip".into();
    let updated = store.update_model(&original_hash, &input).await.unwrap();
    assert_eq!(updated.model_hash, original_hash);
    assert_eq!(updated.sort_order, 3);
    assert_eq!(updated.tooltip_data, "Updated tooltip");
}

#[tokio::test]
async fn arbitrary_request_url_is_independent_from_openai_protocol() {
    let (_directory, store) = store().await;
    let mut input = model_input();
    input.base_url = "https://proxy.example.com/arbitrary/generate?api-version=2026-01-01".into();

    let chat = store.create_model(&input).await.unwrap();
    assert_eq!(chat.provider_type(), ProviderType::OpenAiChat);
    assert_eq!(chat.request_url().unwrap(), input.base_url);

    input.openai_endpoint = "/v1/responses".into();
    let responses = store.update_model(&chat.model_hash, &input).await.unwrap();
    assert_eq!(responses.provider_type(), ProviderType::OpenAiResponses);
    assert_eq!(responses.request_url().unwrap(), input.base_url);
}

#[tokio::test]
async fn standard_server_address_resolves_to_the_same_model_identity_as_a_complete_url() {
    let (_directory, store) = store().await;
    let complete = model_input();
    let mut standard = complete.clone();
    standard.base_url = "https://example.com/v1".into();
    standard.use_full_url = false;

    let model = store.create_model(&standard).await.unwrap();
    assert!(!model.use_full_url);
    assert_eq!(
        model.request_url().unwrap(),
        "https://example.com/v1/chat/completions"
    );
    assert_eq!(
        model.model_hash,
        cursor_server::model::model_hash(&complete).unwrap()
    );
}

#[tokio::test]
async fn call_summary_is_always_stored_and_payloads_follow_detailed_setting() {
    let (_directory, store) = store().await;
    let model = store.create_model(&model_input()).await.unwrap();
    let call = NewLlmCall {
        call_id: "call-1".into(),
        run_id: "run-1".into(),
        conversation_id: "conversation-1".into(),
        provider_call_index: 0,
        model_hash: model.model_hash,
        provider_type: ProviderType::OpenAiChat,
        provider_url: model.base_url.clone(),
        request_type: ProviderType::OpenAiChat,
        request_url: "https://example.com/v1/chat/completions".into(),
        model_id: model.model_id,
        display_name: model.display_name,
        reasoning_effort: Some("high".into()),
        fast: true,
        message_count: 2,
        tool_count: 3,
        detailed: false,
    };
    let conversation_id = ConversationId::new("conversation-1");
    let base_revision_id = store.ensure_conversation(&conversation_id).await.unwrap();
    store
        .claim_run(&PreparedRun {
            run_id: RunId::new("run-1"),
            cursor_request_id: None,
            conversation_id,
            kind: RunKind::Root,
            model: ModelSpec::new(call.model_hash.clone()),
            prompt: PromptSpec {
                instructions: String::new(),
                tools: Vec::new(),
            },
            initial_messages: Vec::new(),
            action: RunAction::Resume {
                pending_tool_round: None,
            },
            base_revision_id,
        })
        .await
        .unwrap();
    store.start_llm_call(&call).await.unwrap();
    store
        .record_llm_request(
            "call-1",
            &serde_json::json!({}),
            &serde_json::json!({"model":"model-a"}),
            false,
        )
        .await
        .unwrap();
    store
        .record_llm_chunk("call-1", 0, 4, b"data", false)
        .await
        .unwrap();
    store
        .record_llm_usage(
            "call-1",
            Usage {
                input_tokens: Some(10),
                output_tokens: Some(5),
                total_tokens: Some(15),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    store
        .finish_llm_call("call-1", "completed", Some("stop"), 9, None, None)
        .await
        .unwrap();

    let summary = store.llm_call("call-1").await.unwrap().unwrap();
    assert_eq!(summary.total_tokens, Some(15));
    assert_eq!(summary.reasoning_effort.as_deref(), Some("high"));
    assert_eq!(summary.fast, Some(true));
    assert_eq!(summary.request_bytes, Some(19));
    assert_eq!(summary.response_bytes, 4);
    assert!(store.llm_call_request("call-1").await.unwrap().is_none());
    assert!(store.llm_call_chunks("call-1").await.unwrap().is_empty());

    let abandoned = NewLlmCall {
        call_id: "call-2".into(),
        provider_call_index: 1,
        ..call
    };
    store.start_llm_call(&abandoned).await.unwrap();
    store
        .finish_run(&RunId::new("run-1"), RunStatus::Cancelled, None, None)
        .await
        .unwrap();
    let abandoned = store.llm_call("call-2").await.unwrap().unwrap();
    assert_eq!(abandoned.status, "cancelled");
    assert!(abandoned.finished_at_ms.is_some());
    assert!(abandoned.duration_ms.is_some());
    assert_eq!(
        store.llm_call("call-1").await.unwrap().unwrap().status,
        "completed"
    );
}
