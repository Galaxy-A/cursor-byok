use std::time::Duration;

use axum::{http::header, response::IntoResponse, routing::post, Router};
use cursor_server::{
    model::{
        ModelConfigInput, ModelInvocation, ModelRequest, ModelSpec, ModelType, PromptSpec,
        OPENAI_CHAT_ENDPOINT,
    },
    provider::{ModelEvent, Provider, ProviderRouter},
    store::Store,
};
use futures_util::StreamExt;
use tokio_util::sync::CancellationToken;

async fn test_store(name: &str) -> (tempfile::TempDir, Store) {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::connect(&format!(
        "sqlite://{}",
        directory.path().join(name).display()
    ))
    .await
    .unwrap();
    (directory, store)
}

#[tokio::test]
async fn cursor_traces_are_absent_when_detailed_logging_is_disabled() {
    let (_directory, store) = test_store("cursor-trace-disabled.db").await;
    assert!(!store
        .start_cursor_trace_if_detailed(
            "request-disabled",
            Some("conversation"),
            "local_byok",
            Some("model"),
        )
        .await
        .unwrap());
    assert!(store
        .cursor_trace("request-disabled")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn cursor_trace_links_detailed_artifacts_to_the_logical_run() {
    let (_directory, store) = test_store("cursor-trace-enabled.db").await;
    store.set_detailed_logging(true).await.unwrap();
    assert!(store
        .start_cursor_trace_if_detailed(
            "request-enabled",
            Some("conversation"),
            "cursor_official",
            Some("official-model"),
        )
        .await
        .unwrap());
    store
        .append_cursor_trace_artifact(
            "request-enabled",
            "bidi_append_request",
            "cursor_client",
            b"request",
            &serde_json::json!({"append_seqno": 1}),
        )
        .await
        .unwrap();
    store
        .add_cursor_trace_request_bytes("request-enabled", 7)
        .await
        .unwrap();
    store
        .start_cursor_trace_response("request-enabled", 200)
        .await
        .unwrap();
    store
        .add_cursor_trace_response_chunk("request-enabled", "cursor_official", b"response")
        .await
        .unwrap();
    store
        .finish_cursor_trace("request-enabled", None)
        .await
        .unwrap();

    let trace = store
        .cursor_trace("request-enabled")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(trace.route, "cursor_official");
    assert_eq!(trace.status, "completed");
    assert_eq!(trace.request_bytes, 7);
    assert_eq!(trace.response_bytes, 8);
    assert_eq!(trace.response_event_count, 1);
    let artifacts = store
        .cursor_trace_artifacts("request-enabled")
        .await
        .unwrap();
    assert_eq!(artifacts.len(), 2);
    assert_eq!(artifacts[0].artifact_type, "bidi_append_request");
    assert_eq!(artifacts[1].artifact_type, "run_sse_chunk");
    assert_eq!(artifacts[1].data, b"response");
}

#[tokio::test]
async fn cursor_trace_artifact_and_blob_are_written_atomically() {
    let (_directory, store) = test_store("cursor-trace-atomic.db").await;
    store.set_detailed_logging(true).await.unwrap();
    store
        .start_cursor_trace_if_detailed(
            "request-atomic",
            Some("conversation"),
            "cursor_official",
            Some("model"),
        )
        .await
        .unwrap();
    sqlx::query(
        "CREATE TRIGGER reject_trace_artifact
         BEFORE INSERT ON cursor_run_trace_artifacts
         BEGIN
             SELECT RAISE(ABORT, 'rejected artifact');
         END",
    )
    .execute(store.pool())
    .await
    .unwrap();

    assert!(store
        .append_cursor_trace_artifact(
            "request-atomic",
            "run_sse_chunk",
            "cursor_official",
            b"must-rollback",
            &serde_json::json!({}),
        )
        .await
        .is_err());

    let blob_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM blobs")
        .fetch_one(store.pool())
        .await
        .unwrap();
    assert_eq!(blob_count, 0);
}

#[tokio::test]
async fn records_one_summary_and_raw_payloads_for_one_provider_request() {
    let app = Router::new().route(
        "/proxy/generate",
        post(|| async {
            (
                [(header::CONTENT_TYPE, "text/event-stream")],
                concat!(
                    "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"},\"finish_reason\":null}]}\n\n",
                    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":2,\"total_tokens\":12}}\n\n",
                    "data: [DONE]\n\n"
                ),
            )
                .into_response()
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let (_directory, store) = test_store("observability.db").await;
    store.set_detailed_logging(true).await.unwrap();
    let model = store
        .create_model(&ModelConfigInput {
            sort_order: 0,
            display_name: "Display Model".into(),
            model_type: ModelType::OpenAi,
            base_url: format!("http://{address}/proxy/generate"),
            use_full_url: true,
            api_key: "not-recorded".into(),
            tooltip_data: "Display Model".into(),
            model_id: "actual-model".into(),
            reasoning_effort: None,
            openai_endpoint: OPENAI_CHAT_ENDPOINT.into(),
            codex_outbound_enabled: false,
            openai_extra_params_enabled: false,
            openai_extra_params: serde_json::json!({}),
            custom_headers_enabled: true,
            custom_headers: serde_json::json!({"x-safe":"visible","authorization":"hidden"}),
            anthropic_extra_params_enabled: false,
            anthropic_extra_params: serde_json::json!({}),
            context_window_tokens: None,
            max_completion_tokens: None,
            anthropic_max_tokens: None,
            anthropic_thinking_effort: None,
            thinking_budget_tokens: None,
        })
        .await
        .unwrap();
    let provider = ProviderRouter::new(store.clone(), Duration::from_secs(5));
    let events = provider
        .stream(
            ModelInvocation {
                call_id: "call-1".into(),
                run_id: "run-1".into(),
                conversation_id: "conversation-1".into(),
                provider_call_index: 0,
                request: ModelRequest {
                    prompt: PromptSpec {
                        instructions: "system".into(),
                        tools: Vec::new(),
                    },
                    model: ModelSpec::new(model.model_hash),
                    history: Vec::new(),
                },
            },
            CancellationToken::new(),
        )
        .collect::<Vec<_>>()
        .await;
    assert!(events.iter().all(Result::is_ok));
    assert!(events
        .iter()
        .any(|event| matches!(event, Ok(ModelEvent::Done(_)))));

    let call = store.llm_call("call-1").await.unwrap().unwrap();
    assert_eq!(call.status, "completed");
    assert_eq!(call.request_type, "openai-chat");
    assert_eq!(call.request_url, format!("http://{address}/proxy/generate"));
    assert_eq!(call.total_tokens, Some(12));
    assert!(call.ttfb_ms.is_some());
    assert!(call.ttft_ms.is_some());
    let request = store.llm_call_request("call-1").await.unwrap().unwrap();
    assert_eq!(request.body["model"], "actual-model");
    assert_eq!(request.headers["x-safe"], "visible");
    assert!(request.headers.get("authorization").is_none());
    assert!(!store.llm_call_chunks("call-1").await.unwrap().is_empty());
    server.abort();
}
