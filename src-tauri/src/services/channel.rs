//! 渠道校验与测试流程，可由桌面 command 或其他后端入口复用。
use sqlx::SqlitePool;

use crate::adapter::{self, Adapter, ChannelConfig, TestResult};
use crate::db::channel as repository;
use crate::error::{AppError, AppResult};
use crate::models::channel::{Channel, CreateChannelInput, UpdateChannelInput};

fn non_empty(field: &str, value: &str) -> AppResult<()> {
    if value.trim().is_empty() {
        return Err(AppError::InvalidInput(format!("{field} cannot be empty")));
    }
    Ok(())
}

fn validate_url(value: &str) -> AppResult<()> {
    let valid = reqwest::Url::parse(value)
        .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some());
    if !valid {
        return Err(AppError::InvalidInput(
            "base_url must be an absolute HTTP(S) URL".into(),
        ));
    }
    Ok(())
}

fn validate_weight(weight: Option<i64>) -> AppResult<()> {
    if weight.is_some_and(|weight| weight < 1) {
        return Err(AppError::InvalidInput("weight must be positive".into()));
    }
    Ok(())
}

fn validate_models(models: &[String]) -> AppResult<()> {
    for model in models {
        non_empty("model", model)?;
    }
    Ok(())
}

pub async fn create_channel(pool: &SqlitePool, input: CreateChannelInput) -> AppResult<Channel> {
    non_empty("name", &input.name)?;
    non_empty("api_key", &input.api_key)?;
    adapter::validate_channel_type(&input.channel_type)?;
    validate_url(&input.base_url)?;
    validate_weight(input.weight)?;
    validate_models(&input.models)?;
    repository::create_channel(pool, input).await
}

pub async fn update_channel(pool: &SqlitePool, input: UpdateChannelInput) -> AppResult<Channel> {
    if let Some(name) = &input.name {
        non_empty("name", name)?;
    }
    if let Some(api_key) = &input.api_key {
        non_empty("api_key", api_key)?;
    }
    if let Some(channel_type) = &input.channel_type {
        adapter::validate_channel_type(channel_type)?;
    }
    if let Some(base_url) = &input.base_url {
        validate_url(base_url)?;
    }
    if let Some(models) = &input.models {
        validate_models(models)?;
    }
    if input.status.is_some_and(|status| !matches!(status, 0 | 1)) {
        return Err(AppError::InvalidInput("status must be 0 or 1".into()));
    }
    validate_weight(input.weight)?;
    repository::update_channel(pool, input).await
}

pub async fn test_channel(
    pool: &SqlitePool,
    client: &reqwest::Client,
    id: &str,
) -> AppResult<TestResult> {
    let channel = repository::find_channel(pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound(id.to_owned()))?;
    let adapter = adapter::get_adapter(&channel.info.channel_type, client)?;
    run_test(pool, channel, adapter.as_ref()).await
}

async fn run_test(
    pool: &SqlitePool,
    channel: Channel,
    adapter: &dyn Adapter,
) -> AppResult<TestResult> {
    let config = ChannelConfig {
        base_url: channel.info.base_url,
        api_key: channel.api_key,
        models: channel.info.models,
        model_mapping: channel.info.model_mapping,
        extra: serde_json::Value::Object(channel.info.config),
    };
    let outcome = adapter.test(&config).await;
    // 内部测试错误也记为失败，避免列表继续显示上一次的成功结果。
    let success = outcome.as_ref().is_ok_and(|result| result.success);
    repository::update_channel_test_result(pool, &channel.info.id, success).await?;
    outcome.map_err(Into::into)
}

#[cfg(test)]
mod tests;
