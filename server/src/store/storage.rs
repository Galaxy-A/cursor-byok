//! Row accounting and cleanup for disposable observability data.

use serde::{Deserialize, Serialize};

use crate::Result;

use super::Store;

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct StatisticsStorage {
    pub call_count: i64,
    pub trace_count: i64,
}

#[cfg(test)]
mod tests {
    use super::Store;
    use crate::model::{NewLlmCall, ProviderType};
    use serde_json::json;

    #[tokio::test]
    async fn statistics_storage_reports_zero_counts_without_byte_estimates() {
        let store = Store::connect("sqlite::memory:").await.unwrap();
        let statistics = store.statistics_storage().await.unwrap();
        assert_eq!(
            serde_json::to_value(statistics).unwrap(),
            json!({ "call_count": 0, "trace_count": 0 })
        );
    }

    #[tokio::test]
    async fn statistics_storage_preserves_counts_for_details_and_resets_them_for_all() {
        let store = Store::connect("sqlite::memory:").await.unwrap();
        store
            .start_llm_call(&NewLlmCall {
                call_id: "call-1".into(),
                run_id: "run-1".into(),
                conversation_id: "conversation-1".into(),
                provider_call_index: 0,
                model_hash: "model-1".into(),
                provider_type: ProviderType::OpenAiChat,
                provider_url: "https://example.com".into(),
                request_type: ProviderType::OpenAiChat,
                request_url: "https://example.com/v1/chat/completions".into(),
                model_id: "model-1".into(),
                display_name: "Model".into(),
                reasoning_effort: None,
                fast: false,
                message_count: 1,
                tool_count: 0,
                detailed: true,
            })
            .await
            .unwrap();
        store
            .record_llm_request("call-1", &json!({}), &json!({"input": "test"}), true)
            .await
            .unwrap();
        store.set_detailed_logging(true).await.unwrap();
        for request_id in ["trace-1", "trace-2"] {
            assert!(store
                .start_cursor_trace_if_detailed(request_id, None, "local_byok", None)
                .await
                .unwrap());
            store
                .append_cursor_trace_artifact(
                    request_id,
                    "request",
                    "cursor_client",
                    request_id.as_bytes(),
                    &json!({}),
                )
                .await
                .unwrap();
        }
        let expected = json!({ "call_count": 1, "trace_count": 2 });
        assert_eq!(
            serde_json::to_value(store.statistics_storage().await.unwrap()).unwrap(),
            expected
        );
        assert_eq!(
            serde_json::to_value(store.clear_statistics_storage().await.unwrap()).unwrap(),
            expected
        );
        let details: i64 = sqlx::query_scalar(
            "SELECT (SELECT COUNT(*) FROM llm_call_requests) +
                    (SELECT COUNT(*) FROM cursor_run_trace_artifacts)",
        )
        .fetch_one(store.pool())
        .await
        .unwrap();
        assert_eq!(details, 0);
        assert_eq!(
            serde_json::to_value(store.clear_all_statistics_storage().await.unwrap()).unwrap(),
            json!({ "call_count": 0, "trace_count": 0 })
        );
        assert!(store.detailed_logging().await.unwrap());
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StatisticsStorageScope {
    #[default]
    Details,
    All,
}

impl Store {
    pub async fn statistics_storage(&self) -> Result<StatisticsStorage> {
        let (call_count, trace_count) = sqlx::query_as::<_, (i64, i64)>(
            "SELECT (SELECT COUNT(*) FROM llm_calls), (SELECT COUNT(*) FROM cursor_run_traces)",
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(StatisticsStorage {
            call_count,
            trace_count,
        })
    }

    pub async fn clear_statistics_storage(&self) -> Result<StatisticsStorage> {
        let _write = self.writes.lock().await;
        let mut transaction = self.pool.begin().await?;
        Self::clear_detail_storage_tx(&mut transaction).await?;
        transaction.commit().await?;
        self.statistics_storage().await
    }

    pub async fn clear_all_statistics_storage(&self) -> Result<StatisticsStorage> {
        let _write = self.writes.lock().await;
        let mut transaction = self.pool.begin().await?;
        Self::clear_trace_artifacts_tx(&mut transaction).await?;
        sqlx::query("DELETE FROM llm_calls")
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM cursor_run_traces")
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        self.statistics_storage().await
    }

    async fn clear_detail_storage_tx(
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ) -> Result<()> {
        sqlx::query("DELETE FROM llm_call_requests")
            .execute(&mut **transaction)
            .await?;
        sqlx::query("DELETE FROM llm_call_response_chunks")
            .execute(&mut **transaction)
            .await?;
        Self::clear_trace_artifacts_tx(transaction).await
    }

    async fn clear_trace_artifacts_tx(
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ) -> Result<()> {
        sqlx::query(
            "CREATE TEMP TABLE IF NOT EXISTS clear_statistics_blob_ids(
                blob_id BLOB PRIMARY KEY
             )",
        )
        .execute(&mut **transaction)
        .await?;
        sqlx::query("DELETE FROM clear_statistics_blob_ids")
            .execute(&mut **transaction)
            .await?;
        sqlx::query(
            "INSERT OR IGNORE INTO clear_statistics_blob_ids(blob_id)
             SELECT blob_id FROM cursor_run_trace_artifacts",
        )
        .execute(&mut **transaction)
        .await?;
        sqlx::query("DELETE FROM cursor_run_trace_artifacts")
            .execute(&mut **transaction)
            .await?;
        sqlx::query(
            "DELETE FROM blobs
             WHERE blob_id IN (SELECT blob_id FROM clear_statistics_blob_ids)
               AND NOT EXISTS (
                   SELECT 1 FROM cursor_run_trace_artifacts a WHERE a.blob_id = blobs.blob_id
               )
               AND NOT EXISTS (
                   SELECT 1 FROM blob_edges e
                   WHERE e.parent_blob_id = blobs.blob_id OR e.child_blob_id = blobs.blob_id
               )",
        )
        .execute(&mut **transaction)
        .await?;
        sqlx::query("DROP TABLE clear_statistics_blob_ids")
            .execute(&mut **transaction)
            .await?;
        Ok(())
    }
}
