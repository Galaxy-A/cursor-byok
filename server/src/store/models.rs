use std::{collections::HashSet, str::FromStr};

use sqlx::{Row, Sqlite, Transaction};

use crate::{
    model::{model_hash, normalize_model_input, ModelConfig, ModelConfigInput, ModelType},
    Error, Result,
};

use super::{now_ms, Store};

const MODEL_COLUMNS: &str = r#"
    model_hash, sort_order, display_name, model_type, base_url, use_full_url, api_key, tooltip_data,
    model_id, reasoning_effort, openai_endpoint, codex_outbound_enabled, openai_extra_params_enabled,
    openai_extra_params_json, custom_headers_enabled, custom_headers_json,
    anthropic_extra_params_enabled, anthropic_extra_params_json, context_window_tokens,
    max_completion_tokens, anthropic_max_tokens, anthropic_thinking_effort,
    thinking_budget_tokens, created_at_ms, updated_at_ms
"#;

impl Store {
    pub async fn models(&self) -> Result<Vec<ModelConfig>> {
        let query =
            format!("SELECT {MODEL_COLUMNS} FROM model_configs ORDER BY sort_order, display_name");
        sqlx::query(&query)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(model_from_row)
            .collect()
    }

    pub async fn model(&self, hash: &str) -> Result<Option<ModelConfig>> {
        let query = format!("SELECT {MODEL_COLUMNS} FROM model_configs WHERE model_hash = ?");
        sqlx::query(&query)
            .bind(hash)
            .fetch_optional(&self.pool)
            .await?
            .map(model_from_row)
            .transpose()
    }

    pub async fn create_model(&self, input: &ModelConfigInput) -> Result<ModelConfig> {
        let mut models = self.create_models(std::slice::from_ref(input)).await?;
        Ok(models.remove(0))
    }

    pub async fn create_models(&self, inputs: &[ModelConfigInput]) -> Result<Vec<ModelConfig>> {
        if inputs.is_empty() {
            return Err(Error::Config("at least one model is required".into()));
        }
        let mut normalized = Vec::with_capacity(inputs.len());
        let mut hashes = HashSet::with_capacity(inputs.len());
        for input in inputs {
            let input = normalize_model_input(input)?;
            let hash = model_hash(&input)?;
            if !hashes.insert(hash.clone()) {
                return Err(Error::Config("model configurations must be unique".into()));
            }
            normalized.push((hash, input));
        }
        let now = now_ms();
        let _write = self.writes.lock().await;
        let mut transaction = self.pool.begin().await?;
        for (hash, input) in &normalized {
            insert_model(&mut transaction, hash, input, now).await?;
        }
        transaction.commit().await?;

        let mut saved = Vec::with_capacity(normalized.len());
        for (hash, _) in normalized {
            saved.push(self.model(&hash).await?.expect("inserted model must exist"));
        }
        Ok(saved)
    }

    pub(super) async fn create_models_if_missing(
        &self,
        inputs: &[ModelConfigInput],
    ) -> Result<usize> {
        let mut normalized = Vec::with_capacity(inputs.len());
        let mut hashes = HashSet::with_capacity(inputs.len());
        for input in inputs {
            let input = normalize_model_input(input)?;
            let hash = model_hash(&input)?;
            if hashes.insert(hash.clone()) {
                normalized.push((hash, input));
            }
        }
        let now = now_ms();
        let _write = self.writes.lock().await;
        let mut transaction = self.pool.begin().await?;
        let mut inserted = 0;
        for (hash, input) in &normalized {
            inserted += usize::from(
                insert_model_with_conflict(&mut transaction, hash, input, now, true).await?,
            );
        }
        transaction.commit().await?;
        Ok(inserted)
    }

    pub async fn update_model(
        &self,
        current_hash: &str,
        input: &ModelConfigInput,
    ) -> Result<ModelConfig> {
        let current = self
            .model(current_hash)
            .await?
            .ok_or_else(|| Error::RunNotFound(format!("model {current_hash}")))?;
        let input = normalize_model_input(input)?;
        let next_hash = model_hash(&input)?;
        let now = now_ms();
        let _write = self.writes.lock().await;
        let mut transaction = self.pool.begin().await?;
        if next_hash != current.model_hash {
            sqlx::query("UPDATE llm_calls SET model_hash = NULL WHERE model_hash = ?")
                .bind(&current.model_hash)
                .execute(&mut *transaction)
                .await?;
        }
        let result = sqlx::query(
            r#"UPDATE model_configs SET
                model_hash = ?, sort_order = ?, display_name = ?, model_type = ?, base_url = ?,
                use_full_url = ?, api_key = ?, tooltip_data = ?, model_id = ?, reasoning_effort = ?,
                openai_endpoint = ?, codex_outbound_enabled = ?, openai_extra_params_enabled = ?, openai_extra_params_json = ?,
                custom_headers_enabled = ?, custom_headers_json = ?,
                anthropic_extra_params_enabled = ?, anthropic_extra_params_json = ?,
                context_window_tokens = ?, max_completion_tokens = ?, anthropic_max_tokens = ?,
                anthropic_thinking_effort = ?, thinking_budget_tokens = ?, updated_at_ms = ?
            WHERE model_hash = ?"#,
        )
        .bind(&next_hash)
        .bind(input.sort_order)
        .bind(&input.display_name)
        .bind(input.model_type.as_str())
        .bind(&input.base_url)
        .bind(input.use_full_url)
        .bind(&input.api_key)
        .bind(&input.tooltip_data)
        .bind(&input.model_id)
        .bind(&input.reasoning_effort)
        .bind(&input.openai_endpoint)
        .bind(input.codex_outbound_enabled)
        .bind(input.openai_extra_params_enabled)
        .bind(serde_json::to_string(&input.openai_extra_params)?)
        .bind(input.custom_headers_enabled)
        .bind(serde_json::to_string(&input.custom_headers)?)
        .bind(input.anthropic_extra_params_enabled)
        .bind(serde_json::to_string(&input.anthropic_extra_params)?)
        .bind(input.context_window_tokens.map(to_i64).transpose()?)
        .bind(input.max_completion_tokens.map(to_i64).transpose()?)
        .bind(input.anthropic_max_tokens.map(to_i64).transpose()?)
        .bind(&input.anthropic_thinking_effort)
        .bind(input.thinking_budget_tokens.map(to_i64).transpose()?)
        .bind(now)
        .bind(current_hash)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() != 1 {
            return Err(Error::RunNotFound(format!("model {current_hash}")));
        }
        transaction.commit().await?;
        Ok(self
            .model(&next_hash)
            .await?
            .expect("updated model must exist"))
    }

    pub async fn delete_model(&self, hash: &str) -> Result<()> {
        let _write = self.writes.lock().await;
        let mut transaction = self.pool.begin().await?;
        sqlx::query("UPDATE llm_calls SET model_hash = NULL WHERE model_hash = ?")
            .bind(hash)
            .execute(&mut *transaction)
            .await?;
        let result = sqlx::query("DELETE FROM model_configs WHERE model_hash = ?")
            .bind(hash)
            .execute(&mut *transaction)
            .await?;
        if result.rows_affected() != 1 {
            return Err(Error::RunNotFound(format!("model {hash}")));
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn reorder_models(&self, model_hashes: &[String]) -> Result<Vec<ModelConfig>> {
        let current = self.models().await?;
        let current_hashes = current
            .iter()
            .map(|model| model.model_hash.as_str())
            .collect::<HashSet<_>>();
        let requested_hashes = model_hashes
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        if model_hashes.len() != current.len()
            || requested_hashes.len() != current.len()
            || requested_hashes != current_hashes
        {
            return Err(Error::Config(
                "model configuration changed; refresh and try sorting again".into(),
            ));
        }

        let now = now_ms();
        let _write = self.writes.lock().await;
        let mut transaction = self.pool.begin().await?;
        for (index, hash) in model_hashes.iter().enumerate() {
            sqlx::query(
                "UPDATE model_configs SET sort_order = ?, updated_at_ms = ? WHERE model_hash = ?",
            )
            .bind(i64::try_from(index + 1).expect("model order fits in i64"))
            .bind(now)
            .bind(hash)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        self.models().await
    }
}

async fn insert_model(
    transaction: &mut Transaction<'_, Sqlite>,
    hash: &str,
    input: &ModelConfigInput,
    now: i64,
) -> Result<()> {
    insert_model_with_conflict(transaction, hash, input, now, false).await?;
    Ok(())
}

async fn insert_model_with_conflict(
    transaction: &mut Transaction<'_, Sqlite>,
    hash: &str,
    input: &ModelConfigInput,
    now: i64,
    ignore_existing: bool,
) -> Result<bool> {
    let mut statement = String::from(
        r#"INSERT INTO model_configs(
            model_hash, sort_order, display_name, model_type, base_url, use_full_url, api_key, tooltip_data,
            model_id, reasoning_effort, openai_endpoint, codex_outbound_enabled, openai_extra_params_enabled,
            openai_extra_params_json, custom_headers_enabled, custom_headers_json,
            anthropic_extra_params_enabled, anthropic_extra_params_json, context_window_tokens,
            max_completion_tokens, anthropic_max_tokens, anthropic_thinking_effort,
            thinking_budget_tokens, created_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    );
    if ignore_existing {
        statement.push_str(" ON CONFLICT(model_hash) DO NOTHING");
    }
    let result = sqlx::query(&statement)
        .bind(hash)
        .bind(input.sort_order)
        .bind(&input.display_name)
        .bind(input.model_type.as_str())
        .bind(&input.base_url)
        .bind(input.use_full_url)
        .bind(&input.api_key)
        .bind(&input.tooltip_data)
        .bind(&input.model_id)
        .bind(&input.reasoning_effort)
        .bind(&input.openai_endpoint)
        .bind(input.codex_outbound_enabled)
        .bind(input.openai_extra_params_enabled)
        .bind(serde_json::to_string(&input.openai_extra_params)?)
        .bind(input.custom_headers_enabled)
        .bind(serde_json::to_string(&input.custom_headers)?)
        .bind(input.anthropic_extra_params_enabled)
        .bind(serde_json::to_string(&input.anthropic_extra_params)?)
        .bind(input.context_window_tokens.map(to_i64).transpose()?)
        .bind(input.max_completion_tokens.map(to_i64).transpose()?)
        .bind(input.anthropic_max_tokens.map(to_i64).transpose()?)
        .bind(&input.anthropic_thinking_effort)
        .bind(input.thinking_budget_tokens.map(to_i64).transpose()?)
        .bind(now)
        .bind(now)
        .execute(&mut **transaction)
        .await?;
    Ok(result.rows_affected() == 1)
}

fn model_from_row(row: sqlx::sqlite::SqliteRow) -> Result<ModelConfig> {
    Ok(ModelConfig {
        model_hash: row.try_get("model_hash")?,
        sort_order: row.try_get("sort_order")?,
        display_name: row.try_get("display_name")?,
        model_type: ModelType::from_str(row.try_get("model_type")?)?,
        base_url: row.try_get("base_url")?,
        use_full_url: row.try_get("use_full_url")?,
        api_key: row.try_get("api_key")?,
        tooltip_data: row.try_get("tooltip_data")?,
        model_id: row.try_get("model_id")?,
        reasoning_effort: row.try_get("reasoning_effort")?,
        openai_endpoint: row.try_get("openai_endpoint")?,
        codex_outbound_enabled: row.try_get("codex_outbound_enabled")?,
        openai_extra_params_enabled: row.try_get("openai_extra_params_enabled")?,
        openai_extra_params: serde_json::from_str(
            row.try_get::<String, _>("openai_extra_params_json")?
                .as_str(),
        )?,
        custom_headers_enabled: row.try_get("custom_headers_enabled")?,
        custom_headers: serde_json::from_str(
            row.try_get::<String, _>("custom_headers_json")?.as_str(),
        )?,
        anthropic_extra_params_enabled: row.try_get("anthropic_extra_params_enabled")?,
        anthropic_extra_params: serde_json::from_str(
            row.try_get::<String, _>("anthropic_extra_params_json")?
                .as_str(),
        )?,
        context_window_tokens: optional_u64(&row, "context_window_tokens")?,
        max_completion_tokens: optional_u64(&row, "max_completion_tokens")?,
        anthropic_max_tokens: optional_u64(&row, "anthropic_max_tokens")?,
        anthropic_thinking_effort: row.try_get("anthropic_thinking_effort")?,
        thinking_budget_tokens: optional_u64(&row, "thinking_budget_tokens")?,
        created_at_ms: row.try_get("created_at_ms")?,
        updated_at_ms: row.try_get("updated_at_ms")?,
    })
}

fn optional_u64(row: &sqlx::sqlite::SqliteRow, column: &str) -> Result<Option<u64>> {
    row.try_get::<Option<i64>, _>(column)?
        .map(|value| {
            u64::try_from(value).map_err(|_| Error::Config(format!("{column} cannot be negative")))
        })
        .transpose()
}

fn to_i64(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| Error::Config("token value is too large".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(name: &str) -> ModelConfigInput {
        ModelConfigInput {
            sort_order: 1,
            display_name: name.into(),
            model_type: ModelType::OpenAi,
            base_url: "https://example.com/v1/responses".into(),
            use_full_url: true,
            api_key: "secret".into(),
            tooltip_data: "Example model".into(),
            model_id: "model-a".into(),
            reasoning_effort: Some("high".into()),
            openai_endpoint: "/v1/responses".into(),
            codex_outbound_enabled: false,
            openai_extra_params_enabled: true,
            openai_extra_params: serde_json::json!({"service_tier":"priority"}),
            custom_headers_enabled: true,
            custom_headers: serde_json::json!({"x-client":"cursor-byok"}),
            anthropic_extra_params_enabled: false,
            anthropic_extra_params: serde_json::json!({}),
            context_window_tokens: Some(200_000),
            max_completion_tokens: Some(8_192),
            anthropic_max_tokens: None,
            anthropic_thinking_effort: None,
            thinking_budget_tokens: None,
        }
    }

    #[tokio::test]
    async fn model_configuration_round_trips_and_updates_identity() {
        let store = Store::connect("sqlite::memory:").await.unwrap();
        let created = store.create_model(&input("Model A")).await.unwrap();
        assert_eq!(created.model_hash.len(), 16);
        assert_eq!(created.custom_headers["x-client"], "cursor-byok");
        assert_eq!(store.models().await.unwrap().len(), 1);

        let updated = store
            .update_model(&created.model_hash, &input("Renamed"))
            .await
            .unwrap();
        assert_ne!(updated.model_hash, created.model_hash);
        assert!(store.model(&created.model_hash).await.unwrap().is_none());

        store.delete_model(&updated.model_hash).await.unwrap();
        assert!(store.models().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn batch_creation_is_atomic() {
        let store = Store::connect("sqlite::memory:").await.unwrap();
        let duplicate = input("Model A");
        assert!(store
            .create_models(&[duplicate.clone(), duplicate])
            .await
            .is_err());
        assert!(store.models().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn model_order_is_replaced_atomically() {
        let store = Store::connect("sqlite::memory:").await.unwrap();
        let first = store.create_model(&input("First")).await.unwrap();
        let mut second_input = input("Second");
        second_input.model_id = "model-b".into();
        second_input.sort_order = 2;
        let second = store.create_model(&second_input).await.unwrap();

        let reordered = store
            .reorder_models(&[second.model_hash.clone(), first.model_hash.clone()])
            .await
            .unwrap();
        assert_eq!(reordered[0].model_hash, second.model_hash);
        assert_eq!(reordered[0].sort_order, 1);
        assert_eq!(reordered[1].model_hash, first.model_hash);
        assert_eq!(reordered[1].sort_order, 2);

        assert!(store
            .reorder_models(std::slice::from_ref(&first.model_hash))
            .await
            .is_err());
        assert_eq!(
            store.models().await.unwrap()[0].model_hash,
            second.model_hash
        );
    }
}
