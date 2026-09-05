//! 渠道 SQL 与存储类型转换。JSON 文本只存在于本模块内部。

use serde::Serialize;
use sqlx::{query, query_as, Row, SqlitePool};
use uuid::Uuid;

use super::now_iso;
use crate::error::{AppError, AppResult};
use crate::models::channel::{Channel, ChannelInfo, CreateChannelInput, UpdateChannelInput};

#[derive(sqlx::FromRow)]
struct ChannelRow {
    id: String,
    name: String,
    #[sqlx(rename = "type")]
    channel_type: String,
    base_url: String,
    api_key: String,
    models: String,
    status: i64,
    priority: i64,
    weight: i64,
    config: String,
    model_mapping: String,
    created_at: String,
    updated_at: String,
    last_test_at: Option<String>,
    last_test_ok: Option<i64>,
}

fn from_json<T: serde::de::DeserializeOwned>(field: &'static str, json: &str) -> AppResult<T> {
    serde_json::from_str(json).map_err(|source| AppError::InvalidStoredData { field, source })
}

impl TryFrom<ChannelRow> for Channel {
    type Error = AppError;

    fn try_from(row: ChannelRow) -> AppResult<Self> {
        Ok(Self {
            info: ChannelInfo {
                id: row.id,
                name: row.name,
                channel_type: row.channel_type,
                base_url: row.base_url,
                has_api_key: !row.api_key.is_empty(),
                models: from_json("models", &row.models)?,
                status: row.status,
                priority: row.priority,
                weight: row.weight,
                config: from_json("config", &row.config)?,
                model_mapping: from_json("model_mapping", &row.model_mapping)?,
                created_at: row.created_at,
                updated_at: row.updated_at,
                last_test_at: row.last_test_at,
                last_test_ok: row.last_test_ok,
            },
            api_key: row.api_key,
        })
    }
}

pub async fn count_channels(pool: &SqlitePool) -> AppResult<i64> {
    let row = sqlx::query("SELECT COUNT(*) AS count FROM channels")
        .fetch_one(pool)
        .await
        .map_err(AppError::from)?;

    row.try_get("count").map_err(AppError::from)
}

const CHANNEL_COLUMNS: &str = r#"
    id,
    name,
    "type",
    base_url,
    api_key,
    models,
    status,
    priority,
    weight,
    config,
    model_mapping,
    created_at,
    updated_at,
    last_test_at,
    last_test_ok
"#;

fn to_json<T: Serialize>(value: &T) -> AppResult<String> {
    serde_json::to_string(value).map_err(AppError::from)
}

pub async fn find_channel(pool: &SqlitePool, id: &str) -> AppResult<Option<Channel>> {
    let sql = format!("SELECT {CHANNEL_COLUMNS} FROM channels WHERE id = ?");

    query_as::<_, ChannelRow>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .map(Channel::try_from)
        .transpose()
}

pub async fn list_channels(pool: &SqlitePool) -> AppResult<Vec<Channel>> {
    let sql = format!(
        "SELECT {CHANNEL_COLUMNS} FROM channels \
         ORDER BY priority DESC, created_at DESC"
    );

    query_as::<_, ChannelRow>(&sql)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(Channel::try_from)
        .collect()
}

pub async fn create_channel(pool: &SqlitePool, input: CreateChannelInput) -> AppResult<Channel> {
    let id = Uuid::new_v4().to_string();
    let now = now_iso();

    let models = to_json(&input.models)?;
    let config = to_json(&input.config.unwrap_or_default())?;
    let model_mapping = to_json(&input.model_mapping.unwrap_or_default())?;

    query(
        r#"
        INSERT INTO channels (
            id, name, "type", base_url, api_key, models,
            status, priority, weight, config, model_mapping,
            created_at, updated_at, last_test_at, last_test_ok
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL)
        "#,
    )
    .bind(&id)
    .bind(input.name)
    .bind(input.channel_type)
    .bind(input.base_url)
    .bind(input.api_key)
    .bind(models)
    .bind(1_i64) // 新增渠道默认启用
    .bind(input.priority.unwrap_or(0))
    .bind(input.weight.unwrap_or(1))
    .bind(config)
    .bind(model_mapping)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(AppError::from)?;

    find_channel(pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(id))
}

pub async fn update_channel(pool: &SqlitePool, input: UpdateChannelInput) -> AppResult<Channel> {
    let models = input.models.as_ref().map(to_json).transpose()?;
    let config = input.config.as_ref().map(to_json).transpose()?;
    let model_mapping = input.model_mapping.as_ref().map(to_json).transpose()?;
    let now = now_iso();

    let result = query(
        r#"
        UPDATE channels SET
            name = COALESCE(?, name),
            "type" = COALESCE(?, "type"),
            base_url = COALESCE(?, base_url),
            api_key = COALESCE(?, api_key),
            models = COALESCE(?, models),
            status = COALESCE(?, status),
            priority = COALESCE(?, priority),
            weight = COALESCE(?, weight),
            config = COALESCE(?, config),
            model_mapping = COALESCE(?, model_mapping),
            updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(input.name)
    .bind(input.channel_type)
    .bind(input.base_url)
    .bind(input.api_key)
    .bind(models)
    .bind(input.status)
    .bind(input.priority)
    .bind(input.weight)
    .bind(config)
    .bind(model_mapping)
    .bind(now)
    .bind(&input.id)
    .execute(pool)
    .await
    .map_err(AppError::from)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(input.id));
    }

    find_channel(pool, &input.id)
        .await?
        .ok_or_else(|| AppError::NotFound(input.id))
}

pub async fn delete_channel(pool: &SqlitePool, id: &str) -> AppResult<()> {
    let result = query("DELETE FROM channels WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .map_err(AppError::from)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(id.to_owned()));
    }

    Ok(())
}

pub async fn update_channel_test_result(
    pool: &SqlitePool,
    id: &str,
    success: bool,
) -> AppResult<()> {
    let result = query(
        r#"
        UPDATE channels
        SET last_test_at = ?, last_test_ok = ?
        WHERE id = ?
        "#,
    )
    .bind(now_iso())
    .bind(if success { 1_i64 } else { 0_i64 })
    .bind(id)
    .execute(pool)
    .await
    .map_err(AppError::from)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(id.to_owned()));
    }

    Ok(())
}
