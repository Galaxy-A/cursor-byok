use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT};
use serde_json::Value;
use uuid::Uuid;

use crate::model::ModelInvocation;

pub(crate) const CODEX_CLI_USER_AGENT: &str = "codex_cli_rs/0.144.6";
pub(crate) const CODEX_CLI_ORIGINATOR: &str = "codex_cli_rs";

pub(crate) struct CodexOutboundIdentity {
    pub installation_id: String,
    pub session_id: String,
    pub thread_id: String,
    pub window_id: String,
    pub turn_metadata: String,
}

pub(crate) fn identity(invocation: &ModelInvocation) -> CodexOutboundIdentity {
    let request = &invocation.request;
    let thread_id = stable_uuid("cursor-byok-codex-thread", &invocation.conversation_id);
    let installation_seed = format!(
        "{}\0{}",
        request.model.model_id,
        request.model.display_name.as_deref().unwrap_or_default()
    );
    let installation_id = stable_uuid("cursor-byok-codex-installation", &installation_seed);
    let turn_id = stable_uuid("cursor-byok-codex-turn", &invocation.call_id);
    let window_id = format!("{thread_id}:0");
    let started_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as i64);
    let turn_metadata = serde_json::json!({
        "installation_id": installation_id,
        "session_id": thread_id,
        "thread_id": thread_id,
        "turn_id": turn_id,
        "window_id": window_id,
        "request_kind": "turn",
        "sandbox": "none",
        "turn_started_at_unix_ms": started_at,
    })
    .to_string();
    CodexOutboundIdentity {
        installation_id,
        session_id: thread_id.clone(),
        thread_id,
        window_id,
        turn_metadata,
    }
}

pub(crate) fn apply_headers(headers: &mut HeaderMap, identity: &CodexOutboundIdentity) {
    insert(headers, USER_AGENT, CODEX_CLI_USER_AGENT);
    insert(headers, "originator", CODEX_CLI_ORIGINATOR);
    insert(headers, ACCEPT, "text/event-stream");
    insert(headers, "session-id", &identity.session_id);
    insert(headers, "thread-id", &identity.thread_id);
    insert(headers, "x-client-request-id", &identity.thread_id);
    insert(
        headers,
        "x-codex-installation-id",
        &identity.installation_id,
    );
    insert(headers, "x-codex-window-id", &identity.window_id);
    insert(headers, "x-codex-turn-metadata", &identity.turn_metadata);
}

pub(crate) fn apply_body(body: &mut Value, invocation: &ModelInvocation) {
    let Some(object) = body.as_object_mut() else {
        return;
    };
    let identity = identity(invocation);
    object.insert("stream".into(), Value::Bool(true));
    object.insert("store".into(), Value::Bool(false));
    object.insert("prompt_cache_key".into(), Value::String(identity.thread_id));
    if object
        .get("tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| !tools.is_empty())
    {
        object.insert("parallel_tool_calls".into(), Value::Bool(true));
        object
            .entry("tool_choice")
            .or_insert_with(|| Value::String("auto".into()));
    }
    if let Some(reasoning) = object.get_mut("reasoning").and_then(Value::as_object_mut) {
        reasoning
            .entry("summary")
            .or_insert_with(|| Value::String("auto".into()));
    }
    let include = object
        .entry("include")
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(values) = include.as_array_mut() else {
        *include = Value::Array(Vec::new());
        return;
    };
    if !values
        .iter()
        .any(|value| value.as_str() == Some("reasoning.encrypted_content"))
    {
        values.push(Value::String("reasoning.encrypted_content".into()));
    }
}

fn insert(headers: &mut HeaderMap, name: impl reqwest::header::IntoHeaderName, value: &str) {
    if let Ok(value) = HeaderValue::from_str(value) {
        headers.insert(name, value);
    }
}

fn stable_uuid(kind: &str, seed: &str) -> String {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("{kind}\0{seed}").as_bytes()).to_string()
}
