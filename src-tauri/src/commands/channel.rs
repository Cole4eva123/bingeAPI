//! IPC 入口：提取参数、调用业务或查询函数、统一返回错误。
use crate::adapter::TestResult;
use crate::db::channel as repository;
use crate::error::CommandError;
use crate::models::channel::{ChannelInfo, CreateChannelInput, UpdateChannelInput};
use crate::services::channel as service;
use crate::AppState;

#[tauri::command]
pub fn greet(name: &str) -> String {
    format!("Hello, {name}! You've been greeted from Rust!")
}

#[tauri::command]
pub async fn count_channels(state: tauri::State<'_, AppState>) -> Result<i64, CommandError> {
    repository::count_channels(&state.database.pool)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn list_channels(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ChannelInfo>, CommandError> {
    repository::list_channels(&state.database.pool)
        .await
        .map(|channels| channels.into_iter().map(|channel| channel.info).collect())
        .map_err(Into::into)
}

#[tauri::command]
pub async fn get_channel(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<Option<ChannelInfo>, CommandError> {
    repository::find_channel(&state.database.pool, &id)
        .await
        .map(|channel| channel.map(|channel| channel.info))
        .map_err(Into::into)
}

#[tauri::command]
pub async fn test_channel(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<TestResult, CommandError> {
    service::test_channel(&state.database.pool, &state.http_client, &id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn create_channel(
    state: tauri::State<'_, AppState>,
    input: CreateChannelInput,
) -> Result<ChannelInfo, CommandError> {
    service::create_channel(&state.database.pool, input)
        .await
        .map(|channel| channel.info)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn update_channel(
    state: tauri::State<'_, AppState>,
    input: UpdateChannelInput,
) -> Result<ChannelInfo, CommandError> {
    service::update_channel(&state.database.pool, input)
        .await
        .map(|channel| channel.info)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn delete_channel(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<bool, CommandError> {
    repository::delete_channel(&state.database.pool, &id).await?;
    Ok(true)
}
