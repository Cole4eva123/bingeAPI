use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RequestLog {
    pub id: String,
    pub api_key_id: Option<String>, //表示值可能有一个String,也可能没有值
    pub api_key_name: Option<String>,
    pub channel_id: Option<String>,
    pub channel_name: Option<String>,
    pub model: String,
    pub upstream_model: Option<String>,
    pub mode: String,
    pub status_code: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub duration_ms: i64,
    pub error_message: Option<String>,
    pub is_stream: i64,
    pub is_retry: i64,
    pub created_at: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct LogFilter {
    pub keyword: Option<String>,
    pub channel_name: Option<String>,
    pub model: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Pagination {
    pub limit: i64,
    pub offset: i64,
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            limit: 50,
            offset: 0,
        }
    }
}

pub async fn search_logs(
    pool: &SqlitePool,
    filter: &LogFilter,
    pagination: Pagination,
) -> AppResult<Vec<RequestLog>> {
    let mut q = QueryBuilder::<Sqlite>::new(
        r#"
        SELECT
            id,
            api_key_id,
            api_key_name,
            channel_id,
            channel_name,
            model,
            upstream_model,
            mode,
            status_code,
            prompt_tokens,
            completion_tokens,
            total_tokens,
            duration_ms,
            error_message,
            is_stream,
            is_retry,
            created_at
        FROM request_logs
        WHERE 1 = 1
        "#,
    );

    if let Some(kw) = filter
        .keyword
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let pattern = format!("%{kw}%");

        q.push(" AND (")
            .push("COALESCE(api_key_name, '') LIKE ")
            .push_bind(pattern.clone())
            .push(" OR COALESCE(channel_name, '') LIKE ")
            .push_bind(pattern.clone())
            .push(" OR model LIKE ")
            .push_bind(pattern.clone())
            .push(" OR COALESCE(upstream_model, '') LIKE ")
            .push_bind(pattern.clone())
            .push(" OR COALESCE(error_message, '') LIKE ")
            .push_bind(pattern)
            .push(")");
    }

    if let Some(name) = filter
        .channel_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        q.push(" AND channel_name = ").push_bind(name);
    }

    if let Some(model) = filter
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        q.push(" AND model = ").push_bind(model);
    }

    if let Some(from) = filter
        .date_from
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        q.push(" AND created_at >= ").push_bind(from);
    }

    if let Some(to) = filter
        .date_to
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        q.push(" AND created_at <= ").push_bind(to);
    }

    let safe_limit = pagination.limit.clamp(1, 1000);
    let safe_offset = pagination.offset.max(0);

    q.push(" ORDER BY created_at DESC")
        .push(" LIMIT ")
        .push_bind(safe_limit)
        .push(" OFFSET ")
        .push_bind(safe_offset);

    q.build_query_as::<RequestLog>()
        .fetch_all(pool)
        .await
        .map_err(Into::into)
}
