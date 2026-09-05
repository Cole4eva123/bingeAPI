use serde::{Deserialize, Serialize};
use sqlx::{query_as, SqlitePool};

use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ApiKey {
    pub id: String,
    pub name: String,
    pub key: String,
    pub status: i64,
    pub allowed_models: String,
    pub allowed_channels: String,
    pub quota_limit: i64,
    pub quota_used: i64,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn get_api_key_by_key(pool: &SqlitePool, key: &str) -> AppResult<Option<ApiKey>> {
    query_as::<_, ApiKey>(
        r#"
        SELECT
            id,
            name,
            "key",
            status,
            allowed_models,
            allowed_channels,
            quota_limit,
            quota_used,
            expires_at,
            created_at,
            updated_at
        FROM api_keys
        WHERE "key" = ?
          AND status = 1
        "#,
    )
    .bind(key)
    .fetch_optional(pool)
    .await
    .map_err(Into::into)
}
